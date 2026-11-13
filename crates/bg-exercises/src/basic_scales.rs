//! Basic scales: Basic timing's groove, each note the next step of the key's scale, up
//! and down one octave. [`ScaleTargets`] hands out the notes and judges the pitches
//! played; the rest of the exercise is Basic timing's (`basic_timing::Kind::Scales`).

use std::sync::{Arc, Mutex, MutexGuard};

use bg_audio::SessionIo;
use bg_core::BarPlan;
use bg_core::harmony::Scale;
use bg_core::music::freq_to_midi;
use bg_core::plan::PlannedNote;

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

/// The notes played in turn: root up to the octave and back down, ready to repeat.
pub fn sequence(scale: &Scale) -> Vec<u8> {
    let root_pc = scale.root % 12;
    let root = LOWEST + (root_pc + 12 - LOWEST % 12) % 12;
    let up: Vec<u8> = scale
        .pitch_classes()
        .iter()
        .map(|&pc| root + (pc + 12 - root_pc) % 12)
        .collect();
    let mut seq = up.clone();
    seq.push(root + 12);
    seq.extend(up[1..].iter().rev());
    seq
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
pub fn preview(bars: &[BarPlan], scale: &Scale) -> Vec<Target> {
    let seq = sequence(scale);
    let mut next = 0;
    bars.iter()
        .flat_map(|b| targets(b, &seq, &mut next))
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
    /// Clears `shared` for the new session.
    pub fn new(scale: &Scale, a4: f32, shared: SharedScale) -> Self {
        lock_scale(&shared).targets.clear();
        Self {
            seq: sequence(scale),
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

    #[test]
    fn scales_go_up_an_octave_and_back() {
        let c = Scale {
            root: 0,
            mode: Mode::Ionian,
        };
        assert_eq!(
            sequence(&c),
            [36, 38, 40, 41, 43, 45, 47, 48, 47, 45, 43, 41, 40, 38]
        );
        let e_minor = Scale {
            root: 4,
            mode: Mode::Aeolian,
        };
        assert_eq!(sequence(&e_minor)[..8], [28, 30, 31, 33, 35, 36, 38, 40]);
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
