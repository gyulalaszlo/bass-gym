//! What each view region shows this frame, derived from the model once per frame.
//!
//! Locks: `chords` before `timing` when both are held, and one at a time where possible.

use std::sync::MutexGuard;

use bg_core::harmony::{Chord, Scale};
use bg_core::score::ScoreChord;
use bg_core::scoring::harmony_grade;
use bg_core::{BarPlan, Grade, Matcher, ScoringThresholds, SessionConfig, SessionStats};
use bg_ui::view::ViewPrefs;
use bg_ui::views::review::Review;
use bg_ui::widgets::keyboard::KeyLabels;
use bg_ui::widgets::performance::TimingMeter;
use bg_ui::widgets::results::ResultsTab;

use super::accompaniment::{ChordShared, lock_chords};
use super::data::{BasicChordsData, resolve_progression};
use super::model::Model;
use super::view::per_chord::{PerChord, Row};
use super::view::status::ChordStatus;
use super::view::strip::StripChord;
use crate::timing_session::{TimingShared, judge_time, last_time, lock, timing_meter};

/// How long the last played note stays on the keyboard after it ends, in seconds.
const BASS_HOLD: f64 = 1.5;

pub struct Toolbar<'a> {
    pub running: bool,
    pub has_results: bool,
    /// The chord now and next, while running.
    pub status: Option<ChordStatus>,
    pub session: &'a SessionConfig,
}

pub fn toolbar(m: &Model) -> Toolbar<'_> {
    let f = &m.frame;
    let status = f.running.then(|| {
        let shared = lock_chords(&m.chords);
        let now = f.clock_now;
        let current = shared.spans.iter().find(|s| s.start <= now && now < s.end);
        let next = shared.spans.iter().find(|s| s.start > now);
        let hide = m.data.keyboard.hide_chord_names;
        ChordStatus {
            current: current.map(|c| {
                (
                    m.data.chord_label(&c.chord),
                    (!hide).then(|| c.chord.roman()),
                )
            }),
            next: next.map(|n| m.data.chord_label(&n.chord)),
        }
    });
    Toolbar {
        running: f.running,
        has_results: m.has_results,
        status,
        session: &m.data.session,
    }
}

pub struct Notices<'a> {
    pub error: Option<&'a str>,
    pub no_audio: bool,
}

pub fn notices(m: &Model) -> Notices<'_> {
    Notices {
        error: m.error.as_deref(),
        no_audio: !m.frame.clock_running,
    }
}

/// What the keyboard shows.
pub struct Keyboard {
    pub keys: u32,
    pub labels: KeyLabels,
    /// Sounding notes.
    pub active: Vec<u8>,
    /// The next chord's notes, shortly before it starts.
    pub ghost: Vec<u8>,
    /// The bassist's current / last note (MIDI) and how it fit the chord.
    pub bass: Option<(i32, Option<Grade>)>,
}

/// The beat grid's data: the live / last session (held locked while shown), or a
/// preview of the settings.
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

/// The live part of the exercise area: keyboard, progression, meter and grid.
pub struct Live<'a> {
    pub running: bool,
    pub keyboard: Keyboard,
    pub strip: Vec<StripChord>,
    /// The progression step sounding now.
    pub strip_current: Option<usize>,
    pub meter: TimingMeter,
    pub grid: Grid<'a>,
    /// The time the grid shows.
    pub grid_now: f64,
    /// The chord band: (start, end, label).
    pub grid_chords: Vec<(f64, f64, String)>,
    /// The chords under the live score.
    pub score_chords: Vec<ScoreChord>,
    pub prefs: &'a ViewPrefs,
    pub a4: f32,
    pub key: Scale,
    pub steps_per_beat: u32,
}

/// The progression to show: the running / last one, or a preview of the settings.
fn shown_progression(m: &Model) -> Vec<Chord> {
    if m.has_results {
        let p = lock_chords(&m.chords).progression.clone();
        if !p.is_empty() {
            return p;
        }
    }
    resolve_progression(&m.data, m.run_seed.unwrap_or(m.data.seed)).unwrap_or_default()
}

fn keyboard(m: &Model, now: f64) -> Keyboard {
    let opts = &m.data.keyboard;
    let (active, ghost) = if m.has_results {
        let shared = lock_chords(&m.chords);
        let active = shared
            .piano
            .iter()
            .filter(|n| n.time <= now && now < n.time + n.len)
            .map(|n| n.midi)
            .collect();
        let ghost = if opts.ghost_next {
            let beat = lock(&m.timing)
                .bars
                .iter()
                .rev()
                .find(|b| b.start <= now)
                .map_or(0.5, |b| b.duration / b.beats.max(1) as f64);
            shared
                .spans
                .iter()
                .find(|s| s.start > now && s.start - now <= beat)
                .map(|s| s.voicing.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        (active, ghost)
    } else {
        (Vec::new(), Vec::new())
    };
    let bass = (opts.show_bass && m.has_results)
        .then(|| {
            let t = lock(&m.timing);
            t.matcher.played.last().and_then(|p| {
                let freq = p.freq?;
                let recent = p.offset.is_none_or(|o| now - o < BASS_HOLD);
                recent.then(|| {
                    let midi = bg_core::music::freq_to_midi(freq, m.frame.a4).0;
                    (midi, p.harmony.map(harmony_grade))
                })
            })
        })
        .flatten();
    Keyboard {
        keys: opts.keys,
        labels: opts.labels,
        active,
        ghost,
        bass,
    }
}

pub fn live(m: &Model) -> Live<'_> {
    let f = &m.frame;
    let d = &m.data;
    let now = if f.running {
        f.clock_now
    } else {
        last_time(&m.timing)
    };
    let keyboard = keyboard(m, now);
    let hide = d.keyboard.hide_chord_names;
    let progression = shown_progression(m);
    let strip = progression
        .iter()
        .map(|c| StripChord {
            label: d.chord_label(c),
            roman: if hide { String::new() } else { c.roman() },
        })
        .collect();
    let strip_current = if m.has_results {
        lock_chords(&m.chords)
            .spans
            .iter()
            .find(|s| s.start <= now && now < s.end)
            .map(|s| s.step)
    } else {
        None
    };
    // Before the grid takes the timing lock: the meter locks it too.
    let meter = timing_meter(
        &m.timing,
        m.has_results,
        now,
        f.view.indicator_bars,
        &f.scoring,
    );
    let (grid, grid_now, grid_chords, score_chords) = if m.has_results {
        // One lock at a time: chords first, then timing.
        let (labels, chords) = lock_chords(&m.chords)
            .spans
            .iter()
            .map(|s| {
                let label = d.chord_label(&s.chord);
                let chord = ScoreChord {
                    start: s.start,
                    end: s.end,
                    tones: s.chord.tones(),
                    label: label.clone(),
                };
                ((s.start, s.end, label), chord)
            })
            .unzip();
        (Grid::Session(lock(&m.timing)), now, labels, chords)
    } else {
        let (bars, matcher) = d.preview();
        let labels = if progression.is_empty() {
            Vec::new()
        } else {
            let len = bars.first().map_or(2.0, |b| b.duration) * d.chord_length.bars() as f64;
            (0..8)
                .map(|i| {
                    let c = &progression[i % progression.len()];
                    (i as f64 * len, (i + 1) as f64 * len, d.chord_label(c))
                })
                .collect()
        };
        let start = bars.first().map_or(0.0, |b| b.start);
        (Grid::Preview(bars, matcher), start, labels, Vec::new())
    };
    Live {
        running: f.running,
        keyboard,
        strip,
        strip_current,
        meter,
        grid,
        grid_now,
        grid_chords,
        score_chords,
        prefs: &f.view,
        a4: f.a4,
        key: d.session.key,
        steps_per_beat: d.session.groove.steps_per_beat,
    }
}

/// The settings panels.
pub struct Config<'a> {
    pub data: &'a BasicChordsData,
    pub run_seed: Option<u64>,
    pub editable: bool,
}

pub fn config(m: &Model) -> Config<'_> {
    Config {
        data: &m.data,
        run_seed: m.run_seed,
        editable: !m.frame.running,
    }
}

pub struct Results<'a> {
    pub open: bool,
    /// The tab, when there's a review to show (otherwise there are no tabs).
    pub tab: Option<ResultsTab>,
    pub stats: Option<SessionStats>,
    pub per_chord: Option<PerChord>,
    pub scoring: &'a ScoringThresholds,
    pub review: Option<&'a Review>,
    pub staff_spacing: f32,
}

pub fn results(m: &Model) -> Results<'_> {
    let open = m.show_results && m.has_results;
    let stats = open.then(|| lock(&m.timing).matcher.stats());
    let per_chord = if open {
        let chords = lock_chords(&m.chords);
        let t = lock(&m.timing);
        per_chord(&chords, &t.matcher, &m.data)
    } else {
        None
    };
    Results {
        open,
        tab: m.review.is_some().then_some(m.results_tab),
        stats,
        per_chord,
        scoring: &m.frame.scoring,
        review: m.review.as_ref(),
        staff_spacing: m.frame.view.results_score_size,
    }
}

/// Targets, hits and chord fit per progression step.
pub fn per_chord(chords: &ChordShared, m: &Matcher, d: &BasicChordsData) -> Option<PerChord> {
    if chords.progression.is_empty() || (m.expected.is_empty() && m.played.is_empty()) {
        return None;
    }
    let step_at = |time: f64| {
        chords
            .spans
            .iter()
            .find(|s| s.start <= time && time < s.end)
            .map(|s| s.step)
    };
    let mut rows: Vec<Row> = chords
        .progression
        .iter()
        .map(|c| Row {
            label: d.chord_label(c),
            ..Row::default()
        })
        .collect();
    for (e, o) in m.expected.iter().zip(&m.outcomes) {
        let (Some(g), Some(step)) = (o.grade, step_at(e.time)) else {
            continue;
        };
        rows[step].notes += 1;
        if g.is_hit() {
            rows[step].hits += 1;
        }
    }
    // Every judged note counts for harmony, extras included, under the chord it was
    // judged against.
    for p in &m.played {
        let Some(h) = p.harmony else { continue };
        if let Some(step) = step_at(judge_time(m, p)) {
            rows[step].judged += 1;
            if harmony_grade(h) == Grade::Good {
                rows[step].in_chord += 1;
            }
        }
    }
    Some(PerChord {
        targets: d.scoring.has_targets(),
        fit: d.scoring.chord_tones,
        rows,
    })
}
