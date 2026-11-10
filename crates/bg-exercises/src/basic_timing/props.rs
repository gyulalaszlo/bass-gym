//! What each view region shows this frame, derived from the model once per frame.

use std::sync::MutexGuard;

use bg_core::{BarPlan, Matcher, ScoringThresholds, SessionConfig, SessionStats};
use bg_ui::view::ViewPrefs;
use bg_ui::views::review::Review;
use bg_ui::widgets::performance::TimingMeter;
use bg_ui::widgets::results::ResultsTab;

use super::model::{Model, ViewOptions};
use super::view::status::Status;
use crate::timing_session::{TimingShared, last_time, lock, preview, timing_meter};

pub struct Toolbar<'a> {
    pub running: bool,
    pub has_results: bool,
    pub status: Status,
    pub session: &'a SessionConfig,
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
        has_results: m.has_results,
        status,
        session: &m.data.session,
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
    let (grid, grid_now) = if m.has_results {
        (Grid::Session(lock(&m.shared)), now)
    } else {
        let (bars, matcher) = preview(&m.data.session);
        let start = bars.first().map_or(0.0, |b| b.start);
        (Grid::Preview(bars, matcher), start)
    };
    Main {
        running: f.running,
        meter,
        grid,
        grid_now,
        view: &m.data.view,
        prefs: &f.view,
        a4: f.a4,
        session: &m.data.session,
    }
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
