//! What each view region shows this frame, derived from the model once per frame.

use std::sync::MutexGuard;

use bg_core::score::Spelling;
use bg_core::{BarPlan, Matcher, ScoringThresholds, SessionConfig, SessionStats};
use bg_ui::view::ViewPrefs;
use bg_ui::views::neck::Tuning;

use crate::basic_scales::{CustomScale, ScaleSettings};
use bg_ui::views::performance::TimingMeter;
use bg_ui::views::results::ResultsTab;
use bg_ui::views::review::Review;
use bg_ui::views::tap_tempo;

use super::model::{Model, ViewOptions};
use super::view::status::Status;
use crate::basic_scales::{self, lock_scale};
use crate::timing_session::{TimingShared, last_time, lock, preview, timing_meter};

pub struct Toolbar<'a> {
    pub running: bool,
    pub tap: &'a tap_tempo::State,
    pub has_results: bool,
    pub status: Status,
    pub session: &'a SessionConfig,
    /// Basic scales' menu: its settings and the user's scales.
    pub scales: Option<(ScaleSettings, &'a [CustomScale])>,
}

pub fn toolbar(m: &Model) -> Toolbar<'_> {
    let f = &m.frame;
    let status = if f.running {
        Status::at(&lock(&m.shared).bars, f.clock_now)
    } else if f.clock_running {
        Status::Ready
    } else {
        Status::Hidden
    };
    Toolbar {
        running: f.running,
        tap: &m.tap,
        has_results: m.has_results,
        status,
        session: &m.data.session,
        scales: m.scales().map(|s| (s, m.frame.library.as_slice())),
    }
}

pub struct Notices {
    pub no_audio: bool,
}

pub fn notices(m: &Model) -> Notices {
    Notices {
        no_audio: !m.frame.clock_running,
    }
}

/// The beat grid's data: the live / last session (held locked while shown), or a
/// preview of the configured groove.
pub enum Grid<'a> {
    Session(MutexGuard<'a, TimingShared>),
    Preview(Vec<BarPlan>, Matcher),
}

impl Grid<'_> {
    pub fn bars(&self) -> &[BarPlan] {
        match self {
            Grid::Session(s) => &s.bars,
            Grid::Preview(bars, _) => bars,
        }
    }

    pub fn matcher(&self) -> &Matcher {
        match self {
            Grid::Session(s) => &s.matcher,
            Grid::Preview(_, m) => m,
        }
    }
}

pub struct Main<'a> {
    pub running: bool,
    pub meter: TimingMeter,
    pub grid: Grid<'a>,
    /// The time the grid shows.
    pub grid_now: f64,
    pub view: &'a ViewOptions,
    pub prefs: &'a ViewPrefs,
    pub a4: f32,
    pub session: &'a SessionConfig,
    /// Basic scales' neck.
    pub neck: Option<Neck>,
    /// Basic scales' notes, for the grid's chord band: (start, end, name).
    pub grid_notes: Vec<(f64, f64, String)>,
}

pub struct Neck {
    pub tuning: Tuning,
    pub frets: u8,
    /// The root's pitch class, and the scale's.
    pub root: u8,
    pub tones: [bool; 12],
    pub target: Option<u8>,
    pub next: Option<u8>,
    pub names: [String; 12],
}

pub fn main(m: &Model) -> Main<'_> {
    let f = &m.frame;
    let now = if f.running {
        f.clock_now
    } else {
        last_time(&m.shared)
    };
    // Before the grid takes the lock: the meter locks too.
    let meter = timing_meter(
        &m.shared,
        m.has_results,
        now,
        f.view.indicator_bars,
        &f.scoring,
    );
    let key = m.data.session.key;
    let preview = (!m.has_results).then(|| preview(&m.data.session));
    let seq = m.sequence();
    // Scale targets before the timing lock: never hold both.
    let targets = match (&seq, &preview) {
        (None, _) => Vec::new(),
        (Some(seq), Some((bars, _))) => basic_scales::preview(bars, seq),
        (Some(_), None) => lock_scale(&m.scale).targets.clone(),
    };
    let (grid, grid_now) = match preview {
        None => (Grid::Session(lock(&m.shared)), now),
        Some((bars, matcher)) => {
            let start = bars.first().map_or(0.0, |b| b.start);
            (Grid::Preview(bars, matcher), start)
        }
    };
    let spelling = Spelling::new(Some(key));
    let names: [String; 12] = std::array::from_fn(|pc| spelling.name(pc as u8));
    let grid_notes = targets
        .iter()
        .map(|t| {
            (
                t.time,
                t.time + t.len,
                names[(t.midi % 12) as usize].clone(),
            )
        })
        .collect();
    let neck = m.scales().zip(seq).map(|(s, seq)| {
        let (target, next) = basic_scales::at(&targets, grid_now);
        let root = key.root % 12;
        let mut tones = [false; 12];
        for i in s.intervals(&key, &f.library) {
            tones[((root + i) % 12) as usize] = true;
        }
        Neck {
            tuning: s.tuning,
            frets: frets(s.tuning, &seq),
            root,
            tones,
            target,
            next,
            names,
        }
    });
    Main {
        running: f.running,
        meter,
        grid,
        grid_now,
        view: &m.data.view,
        prefs: &f.view,
        a4: f.a4,
        session: &m.data.session,
        neck,
        grid_notes,
    }
}

/// Frets the neck shows: 12, or 24 when the notes go higher.
fn frets(tuning: Tuning, seq: &[u8]) -> u8 {
    let top = tuning.strings().last().copied().unwrap_or(0);
    let high = seq.iter().max().copied().unwrap_or(0);
    if high > top + 12 { 24 } else { 12 }
}

pub struct Settings<'a> {
    pub session: &'a SessionConfig,
    pub editable: bool,
}

pub fn settings(m: &Model) -> Settings<'_> {
    Settings {
        session: &m.data.session,
        editable: !m.frame.running,
    }
}

pub struct Results<'a> {
    pub open: bool,
    /// The tab, when there's a review to show (otherwise there are no tabs).
    pub tab: Option<ResultsTab>,
    pub stats: Option<SessionStats>,
    pub scoring: &'a ScoringThresholds,
    pub review: Option<&'a Review>,
    pub staff_spacing: f32,
}

pub fn results(m: &Model) -> Results<'_> {
    let open = m.show_results && m.has_results;
    Results {
        open,
        tab: m.review.is_some().then_some(m.results_tab),
        stats: open.then(|| lock(&m.shared).matcher.stats()),
        scoring: &m.frame.scoring,
        review: m.review.as_ref(),
        staff_spacing: m.frame.view.results_score_size,
    }
}
