//! Basic scales: Basic timing's groove, each note the next step of a scale on the key's
//! root, up and back down. [`ScaleTargets`] hands out the notes and judges the pitches
//! played; the rest of the exercise is Basic timing's (`basic_timing::Kind::Scales`).
//! `menu.rs` is its toolbar menu (which also edits the preset's own copy of the scale),
//! `prefs.rs` the library of the user's scales to pick from.

pub mod menu;
pub mod prefs;

use std::cell::RefCell;
use std::sync::{Arc, Mutex, MutexGuard};

use bg_audio::SessionIo;
use bg_core::harmony::Scale;
use bg_core::music::freq_to_midi;
use bg_core::plan::PlannedNote;
use bg_core::{BarPlan, Matcher, SessionConfig};
use bg_ui::views::neck::Tuning;
use serde::{Deserialize, Serialize};

pub use prefs::{ScaleDef, ScaleLibrary};

use crate::timing_session::{self, Accompaniment, AccompanimentScoring};

/// The lowest note a scale starts on: open E.
const LOWEST: u8 = 28;

/// A note to play: a groove note's time, accent and length (to the bar's end when it's
/// held), and its pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub time: f64,
    pub len: f64,
    pub accent: bool,
    pub midi: u8,
}

/// The session's targets, written on the worker thread and read by the UI.
#[derive(Default)]
pub struct ScaleShared {
    pub targets: Vec<Target>,
}

pub type SharedScale = Arc<Mutex<ScaleShared>>;

pub fn lock_scale(shared: &SharedScale) -> MutexGuard<'_, ScaleShared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Scales besides the key's mode: (name, semitones above the root).
pub const BUILT_IN: [(&str, &[u8]); 6] = [
    ("Major pentatonic", &[0, 2, 4, 7, 9]),
    ("Minor pentatonic", &[0, 3, 5, 7, 10]),
    ("Blues", &[0, 3, 5, 6, 7, 10]),
    ("Harmonic minor", &[0, 2, 3, 5, 7, 8, 11]),
    ("Melodic minor", &[0, 2, 3, 5, 7, 9, 11]),
    ("Chromatic", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
];

/// What Basic scales plays, and the neck it shows. In its presets only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScaleSettings {
    /// The scale itself, copied from [`BUILT_IN`] or the user's library so the preset
    /// keeps it and can change it for itself; `None` is the key's mode.
    pub scale: Option<ScaleDef>,
    /// Octaves the scale spans, and how many octaves above the lowest root it starts.
    pub octaves: u8,
    pub start: u8,
    /// Every how-manyth note of the scale is played: 1 all, 2 every other, ...
    pub step: u8,
    pub direction: Direction,
    /// Root to root: every run ends on its root (the top one added when `step` skips
    /// it), held to the end of the bar; the next run starts on the next bar line.
    pub whole_bars: bool,
    pub tuning: Tuning,
}

/// Which way the scale runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Down,
    /// Up, then down, then up again, ...
    #[default]
    UpDown,
}

impl Direction {
    pub const ALL: [Direction; 3] = [Self::Up, Self::Down, Self::UpDown];

    pub fn label(self) -> &'static str {
        match self {
            Self::Up => "Ascending",
            Self::Down => "Descending",
            Self::UpDown => "Both",
        }
    }
}

impl Default for ScaleSettings {
    fn default() -> Self {
        Self {
            scale: None,
            octaves: 1,
            start: 0,
            step: 1,
            direction: Direction::default(),
            whole_bars: false,
            tuning: Tuning::default(),
        }
    }
}

/// The key's mode as semitones above its root.
pub fn mode_intervals(key: &Scale) -> Vec<u8> {
    let root = key.root % 12;
    key.pitch_classes()
        .iter()
        .map(|&pc| (pc + 12 - root) % 12)
        .collect()
}

impl ScaleSettings {
    /// The scale's semitones above the root, ascending from 0: the preset's scale (as
    /// read from a file it may be out of order or range), or the key's mode.
    pub fn intervals(&self, key: &Scale) -> Vec<u8> {
        let Some(def) = &self.scale else {
            return mode_intervals(key);
        };
        let mut v: Vec<u8> = def.intervals.iter().copied().filter(|&i| i < 12).collect();
        v.push(0);
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The scale's name, for the menu label.
    pub fn name(&self, key: &Scale) -> String {
        self.scale
            .as_ref()
            .map_or_else(|| key.mode.label().to_owned(), |s| s.name.clone())
    }

    /// Every `step`th note on `key`'s root over `octaves`, ascending, up to the top root
    /// (always ending on it when [`whole_bars`](Self::whole_bars)).
    pub fn up(&self, key: &Scale) -> Vec<u8> {
        let intervals = self.intervals(key);
        let root_pc = key.root % 12;
        let root = LOWEST + (root_pc + 12 - LOWEST % 12) % 12 + 12 * self.start;
        let span = self.octaves.max(1);
        let top = root + 12 * span;
        let notes = (0..span)
            .flat_map(|o| intervals.iter().map(move |&i| root + 12 * o + i))
            .chain([top]);
        let mut up: Vec<u8> = notes.step_by(self.step.max(1) as usize).collect();
        if self.whole_bars && up.last() != Some(&top) {
            up.push(top);
        }
        up
    }

    /// The runs played in turn, over and over. With [`whole_bars`](Self::whole_bars) each
    /// is one pass root to root and a bar line follows it; otherwise there's one run,
    /// looped without a break (up and down doesn't repeat the turning notes).
    pub fn runs(&self, key: &Scale) -> Vec<Vec<u8>> {
        let up = self.up(key);
        let down: Vec<u8> = up.iter().rev().copied().collect();
        match (self.whole_bars, self.direction) {
            (_, Direction::Up) => vec![up],
            (_, Direction::Down) => vec![down],
            (true, Direction::UpDown) => vec![up, down],
            (false, Direction::UpDown) => {
                let mut run = up.clone();
                if up.len() > 2 {
                    run.extend(&down[1..down.len() - 1]);
                }
                vec![run]
            }
        }
    }
}

/// Gives a session's groove notes their pitches, bar by bar.
pub struct Walker {
    runs: Vec<Vec<u8>>,
    whole_bars: bool,
    run: usize,
    pos: usize,
}

impl Walker {
    pub fn new(settings: &ScaleSettings, key: &Scale) -> Self {
        Self {
            runs: settings.runs(key),
            whole_bars: settings.whole_bars,
            run: 0,
            pos: 0,
        }
    }

    /// The highest note played.
    pub fn highest(&self) -> u8 {
        self.runs.iter().flatten().copied().max().unwrap_or(0)
    }

    /// The targets of `bar` (none in a count-in). A run's last note ends the bar when
    /// runs are root to root: it's held to the bar line, and the groove notes after it
    /// aren't played.
    pub fn bar(&mut self, bar: &BarPlan) -> Vec<Target> {
        let mut out = Vec::new();
        if bar.is_count_in() {
            return out;
        }
        for n in &bar.notes {
            let run = &self.runs[self.run % self.runs.len()];
            let midi = run[self.pos];
            self.pos += 1;
            let last = self.pos >= run.len();
            if last {
                (self.pos, self.run) = (0, self.run + 1);
            }
            let held = last && self.whole_bars;
            out.push(Target {
                time: n.time,
                len: if held { bar.end() - n.time } else { n.len },
                accent: n.accent,
                midi,
            });
            if held {
                break;
            }
        }
        out
    }
}

/// What Basic scales scores: the walker's targets (handed to the session as hits), not
/// the groove's notes, which a held root can leave out; and their pitches.
pub fn scoring() -> AccompanimentScoring {
    AccompanimentScoring {
        groove: false,
        hits: true,
        ..Default::default()
    }
}

/// A preview of `cfg` (bars and matcher, as [`timing_session::preview`]) with `walker`'s
/// targets, which it also returns.
pub fn preview(cfg: &SessionConfig, walker: Walker) -> ((Vec<BarPlan>, Matcher), Vec<Target>) {
    let walker = RefCell::new(walker);
    let targets = RefCell::new(Vec::new());
    let p = timing_session::preview_targets(cfg, false, |b| {
        let t = walker.borrow_mut().bar(b);
        targets.borrow_mut().extend(&t);
        planned(&t)
    });
    (p, targets.into_inner())
}

fn planned(targets: &[Target]) -> Vec<PlannedNote> {
    targets
        .iter()
        .map(|t| PlannedNote {
            time: t.time,
            len: t.len,
            accent: t.accent,
        })
        .collect()
}

/// The note to play at `now`, and the one after it.
pub fn at(targets: &[Target], now: f64) -> (Option<u8>, Option<u8>) {
    let i = targets.partition_point(|t| t.time + t.len / 2.0 <= now);
    (
        targets.get(i).map(|t| t.midi),
        targets.get(i + 1).map(|t| t.midi),
    )
}

/// How well `played` matches `want`: 1 the note, 0.5 the note in another octave, else 0.
/// Graded like harmony: Good, Ok, Poor.
pub fn pitch_score(want: u8, played: i32) -> f32 {
    match played - want as i32 {
        0 => 1.0,
        d if d % 12 == 0 => 0.5,
        _ => 0.0,
    }
}

/// The scale as an accompaniment that plays nothing: its targets are the session's (run
/// with [`scoring`]), and it judges what's played against the nearest one.
pub struct ScaleTargets {
    walker: Walker,
    a4: f32,
    shared: SharedScale,
}

impl ScaleTargets {
    /// Clears `shared` for the new session.
    pub fn new(walker: Walker, a4: f32, shared: SharedScale) -> Self {
        lock_scale(&shared).targets.clear();
        Self { walker, a4, shared }
    }
}

impl Accompaniment for ScaleTargets {
    fn plan_bar(&mut self, bar: &BarPlan, _io: &mut SessionIo) -> Vec<PlannedNote> {
        let t = self.walker.bar(bar);
        lock_scale(&self.shared).targets.extend(&t);
        planned(&t)
    }

    fn judge(&self, time: f64, freq: f32) -> Option<f32> {
        let s = lock_scale(&self.shared);
        let t = s
            .targets
            .iter()
            .min_by(|a, b| (a.time - time).abs().total_cmp(&(b.time - time).abs()))?;
        Some(pitch_score(t.midi, freq_to_midi(freq, self.a4).0))
    }

    fn stop(&mut self, now: f64) {
        lock_scale(&self.shared).targets.retain(|t| t.time <= now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_core::harmony::Mode;

    const C: Scale = Scale {
        root: 0,
        mode: Mode::Ionian,
    };

    /// The single looped run (not root to root).
    fn seq(s: ScaleSettings, key: &Scale) -> Vec<u8> {
        let mut runs = s.runs(key);
        assert_eq!(runs.len(), 1);
        runs.remove(0)
    }

    fn def(name: &str, intervals: &[u8]) -> Option<ScaleDef> {
        Some(ScaleDef {
            name: name.into(),
            intervals: intervals.to_vec(),
        })
    }

    #[test]
    fn scales_go_up_an_octave_and_back() {
        let one = ScaleSettings::default();
        assert_eq!(
            seq(one.clone(), &C),
            [36, 38, 40, 41, 43, 45, 47, 48, 47, 45, 43, 41, 40, 38]
        );
        let e_minor = Scale {
            root: 4,
            mode: Mode::Aeolian,
        };
        assert_eq!(seq(one, &e_minor)[..8], [28, 30, 31, 33, 35, 36, 38, 40]);
    }

    #[test]
    fn patterns_span_octaves_start_higher_and_skip_notes() {
        let pent = |octaves, start, step| ScaleSettings {
            scale: def("Minor pentatonic", &[0, 3, 5, 7, 10]),
            octaves,
            start,
            step,
            ..Default::default()
        };
        // A minor pentatonic over two octaves from A1, up and down.
        let a = Scale { root: 9, ..C };
        let two = seq(pent(2, 0, 1), &a);
        assert_eq!(two[..11], [33, 36, 38, 40, 43, 45, 48, 50, 52, 55, 57]);
        assert_eq!(two.len(), 20);
        assert_eq!(seq(pent(1, 1, 1), &a)[0], 45);
        // Every other note: A C E ... down again without repeating the ends.
        assert_eq!(seq(pent(1, 0, 2), &a), [33, 38, 43, 38]);
        // Off the end of a short scale: just the root.
        assert_eq!(seq(pent(1, 0, 9), &a), [33]);
    }

    #[test]
    fn directions_loop_one_way_or_turn() {
        let dir = |direction| ScaleSettings {
            scale: def("Major pentatonic", &[0, 2, 4, 7, 9]),
            direction,
            ..Default::default()
        };
        assert_eq!(seq(dir(Direction::Up), &C), [36, 38, 40, 43, 45, 48]);
        assert_eq!(seq(dir(Direction::Down), &C), [48, 45, 43, 40, 38, 36]);
        assert_eq!(
            seq(dir(Direction::UpDown), &C),
            [36, 38, 40, 43, 45, 48, 45, 43, 40, 38]
        );
    }

    /// The targets of the first `n` bars of a quarter-note groove at 120 BPM (2 s bars).
    fn walk(s: &ScaleSettings, n: usize) -> Vec<Vec<(f64, f64, u8)>> {
        let cfg = SessionConfig {
            tempo: 120.0,
            ..Default::default()
        };
        let (bars, _) = timing_session::preview(&cfg);
        let mut w = Walker::new(s, &C);
        let start = bars[0].start;
        bars.iter()
            .take(n)
            .map(|b| {
                w.bar(b)
                    .iter()
                    .map(|t| (t.time - start, t.len, t.midi))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn root_to_root_holds_the_root_and_restarts_on_the_bar_line() {
        let whole = |direction| ScaleSettings {
            scale: def("Major pentatonic", &[0, 2, 4, 7, 9]),
            direction,
            whole_bars: true,
            ..Default::default()
        };
        // C D E G | A C'(held 1.5 s) | C D E G ...
        let up = walk(&whole(Direction::Up), 3);
        assert_eq!(
            up[0].iter().map(|t| t.2).collect::<Vec<_>>(),
            [36, 38, 40, 43]
        );
        assert_eq!(up[1], [(2.0, 0.5, 45), (2.5, 1.5, 48)]);
        assert_eq!(up[2][0], (4.0, 0.5, 36));
        // Both: the next bar runs down from the top root.
        let both = walk(&whole(Direction::UpDown), 4);
        assert_eq!(both[2][0].2, 48);
        assert_eq!(both[3], [(6.0, 0.5, 38), (6.5, 1.5, 36)]);
        // Every other note skips the top C: it's added. C E A C'.
        let skip = ScaleSettings {
            step: 2,
            ..whole(Direction::Up)
        };
        assert_eq!(skip.up(&C), [36, 40, 45, 48]);
    }

    #[test]
    fn preset_scales_are_cleaned_up() {
        let named = |iv: &[u8]| ScaleSettings {
            scale: def("Mine", iv),
            ..Default::default()
        };
        assert_eq!(named(&[7, 0]).intervals(&C), [0, 7]);
        assert_eq!(named(&[]).intervals(&C), [0]);
        assert_eq!(named(&[4, 4, 15]).intervals(&C), [0, 4]);
        assert_eq!(
            ScaleSettings::default().intervals(&C),
            [0, 2, 4, 5, 7, 9, 11]
        );
    }

    #[test]
    fn pitches_score_by_note_then_octave() {
        assert_eq!(pitch_score(36, 36), 1.0);
        assert_eq!(pitch_score(36, 48), 0.5);
        assert_eq!(pitch_score(36, 24), 0.5);
        assert_eq!(pitch_score(36, 37), 0.0);
    }

    #[test]
    fn the_target_now_is_the_one_not_yet_half_past() {
        let t = |time, midi| Target {
            time,
            len: 0.5,
            accent: false,
            midi,
        };
        let ts = [t(0.0, 36), t(0.5, 38), t(1.0, 40)];
        assert_eq!(at(&ts, -1.0), (Some(36), Some(38)));
        assert_eq!(at(&ts, 0.3), (Some(38), Some(40)));
        assert_eq!(at(&ts, 1.3), (None, None));
    }
}
