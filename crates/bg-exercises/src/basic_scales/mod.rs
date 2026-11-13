//! Basic scales: Basic timing's groove, each note the next step of a scale on the key's
//! root, up and back down. [`ScaleTargets`] hands out the notes and judges the pitches
//! played; the rest of the exercise is Basic timing's (`basic_timing::Kind::Scales`).
//! `menu.rs` is its toolbar menu (which also edits the preset's own copy of the scale),
//! `prefs.rs` the library of the user's scales to pick from.

pub mod menu;
pub mod prefs;

use std::sync::{Arc, Mutex, MutexGuard};

use bg_audio::SessionIo;
use bg_core::BarPlan;
use bg_core::harmony::Scale;
use bg_core::music::freq_to_midi;
use bg_core::plan::PlannedNote;
use bg_ui::views::neck::Tuning;
use serde::{Deserialize, Serialize};

pub use prefs::{ScaleDef, ScaleLibrary};

use crate::timing_session::Accompaniment;

/// The lowest note a scale starts on: open E.
const LOWEST: u8 = 28;

/// A note to play: a groove note's time and length, and its pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub time: f64,
    pub len: f64,
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
    /// Root to root in whole bars: up to the top root even when `step` skips it, down to
    /// the bottom root, then the root again until the groove's notes are filled, so every
    /// round starts on a bar line.
    pub whole_bars: bool,
    pub tuning: Tuning,
}

impl Default for ScaleSettings {
    fn default() -> Self {
        Self {
            scale: None,
            octaves: 1,
            start: 0,
            step: 1,
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

    /// The notes played in turn on `key`'s root, ready to repeat: every `step`th note up
    /// over `octaves` (the top root included), then back down. `groove_notes` is the
    /// groove's notes per round, which [`whole_bars`](Self::whole_bars) fills.
    pub fn sequence(&self, key: &Scale, groove_notes: usize) -> Vec<u8> {
        let intervals = self.intervals(key);
        let root_pc = key.root % 12;
        let root = LOWEST + (root_pc + 12 - LOWEST % 12) % 12 + 12 * self.start;
        let span = self.octaves.max(1);
        let top = root + 12 * span;
        let notes = (0..span)
            .flat_map(|o| intervals.iter().map(move |&i| root + 12 * o + i))
            .chain([top]);
        let mut up: Vec<u8> = notes.step_by(self.step.max(1) as usize).collect();
        if self.whole_bars {
            if up.last() != Some(&top) {
                up.push(top);
            }
            let mut seq = up.clone();
            seq.extend(up.iter().rev().skip(1));
            if groove_notes > 0 {
                while !seq.len().is_multiple_of(groove_notes) {
                    seq.push(root);
                }
            }
            return seq;
        }
        let mut seq = up.clone();
        if up.len() > 2 {
            seq.extend(up[1..up.len() - 1].iter().rev());
        }
        seq
    }
}

/// The targets of `bar`'s notes, continuing `seq` from step `*next`.
pub fn targets(bar: &BarPlan, seq: &[u8], next: &mut usize) -> Vec<Target> {
    bar.notes
        .iter()
        .map(|n| {
            let midi = seq[*next % seq.len()];
            *next += 1;
            Target {
                time: n.time,
                len: n.len,
                midi,
            }
        })
        .collect()
}

/// The targets of `bars` (a preview: no count-in).
pub fn preview(bars: &[BarPlan], seq: &[u8]) -> Vec<Target> {
    let mut next = 0;
    bars.iter()
        .flat_map(|b| targets(b, seq, &mut next))
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

/// The scale as an accompaniment that plays nothing: it gives each groove note its
/// pitch, and judges what's played against the nearest one.
pub struct ScaleTargets {
    seq: Vec<u8>,
    next: usize,
    a4: f32,
    shared: SharedScale,
}

impl ScaleTargets {
    /// Plays `seq` over and over (see [`ScaleSettings::sequence`]). Clears `shared` for
    /// the new session.
    pub fn new(seq: Vec<u8>, a4: f32, shared: SharedScale) -> Self {
        assert!(!seq.is_empty(), "a scale with no notes");
        lock_scale(&shared).targets.clear();
        Self {
            seq,
            next: 0,
            a4,
            shared,
        }
    }
}

impl Accompaniment for ScaleTargets {
    fn plan_bar(&mut self, bar: &BarPlan, _io: &mut SessionIo) -> Vec<PlannedNote> {
        if !bar.is_count_in() {
            let t = targets(bar, &self.seq, &mut self.next);
            lock_scale(&self.shared).targets.extend(t);
        }
        Vec::new()
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

    fn seq(s: ScaleSettings, key: &Scale) -> Vec<u8> {
        s.sequence(key, 4)
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
    fn whole_bars_go_root_to_root_and_fill_the_groove() {
        let whole = |step| ScaleSettings {
            step,
            whole_bars: true,
            ..Default::default()
        };
        // C major: 15 notes root to root, one more C fills four bars of four.
        let s = seq(whole(1), &C);
        assert_eq!(s.len(), 16);
        assert_eq!(s[7], 48);
        assert_eq!(s[14..], [36, 36]);
        // Every other note skips the top C: it's added. C E G B C' B G E C, then C to 12.
        let s = seq(whole(2), &C);
        assert_eq!(s[..9], [36, 40, 43, 47, 48, 47, 43, 40, 36]);
        assert_eq!(s.len(), 12);
        assert!(s[9..].iter().all(|&n| n == 36));
        // No groove notes: nothing to fill.
        assert_eq!(whole(1).sequence(&C, 0).len(), 15);
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
            midi,
        };
        let ts = [t(0.0, 36), t(0.5, 38), t(1.0, 40)];
        assert_eq!(at(&ts, -1.0), (Some(36), Some(38)));
        assert_eq!(at(&ts, 0.3), (Some(38), Some(40)));
        assert_eq!(at(&ts, 1.3), (None, None));
    }
}
