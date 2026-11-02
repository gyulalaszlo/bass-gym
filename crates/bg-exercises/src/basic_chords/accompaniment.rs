//! The simulated keyboard player: steps through the progression, voices each chord with
//! voice leading, comps it with the selected rhythm and judges the bassist's notes.

use std::sync::{Arc, Mutex, MutexGuard};

use bg_audio::{SessionIo, Slot};
use bg_core::harmony::{Chord, ChordChecker, Scale, comp, voice};
use bg_core::music::freq_to_midi;
use bg_core::plan::BarPlan;
use bg_sound::{SoundEvent, SoundKind};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::timing_session::Accompaniment;

/// A chord as played: when, which progression step, and the piano voicing.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordSpan {
    pub start: f64,
    pub end: f64,
    /// Index into the progression.
    pub step: usize,
    pub chord: Chord,
    pub voicing: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PianoNote {
    pub time: f64,
    pub len: f64,
    pub midi: u8,
}

/// What the piano plays, shared with the UI.
#[derive(Clone, Debug, Default)]
pub struct ChordShared {
    pub progression: Vec<Chord>,
    pub spans: Vec<ChordSpan>,
    pub piano: Vec<PianoNote>,
}

pub type SharedChords = Arc<Mutex<ChordShared>>;

pub fn lock_chords(shared: &SharedChords) -> MutexGuard<'_, ChordShared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct ChordAccompaniment {
    pub progression: Vec<Chord>,
    pub scale: Scale,
    pub checker: &'static dyn ChordChecker,
    pub comp: String,
    /// Chord length as a multiple of the bar (0.5, 1, 2, 4).
    pub bars_per_chord: f32,
    pub jazziness: f32,
    pub volume: f32,
    pub a4: f32,
    pub shared: SharedChords,
    rng: ChaCha8Rng,
    prev_voicing: Option<Vec<u8>>,
    /// Voicing prepared early for an anticipated chord (progression position, notes).
    anticipated: Option<(usize, Vec<u8>)>,
    /// Skip the first hit of the next bar (it was anticipated).
    skip_downbeat: bool,
    /// Progression position of the last span, to extend chords held across bar lines.
    last_pos: Option<usize>,
}

impl ChordAccompaniment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        progression: Vec<Chord>,
        scale: Scale,
        checker: &'static dyn ChordChecker,
        comp: String,
        bars_per_chord: f32,
        jazziness: f32,
        volume: f32,
        a4: f32,
        seed: u64,
        shared: SharedChords,
    ) -> Self {
        *lock_chords(&shared) = ChordShared {
            progression: progression.clone(),
            ..Default::default()
        };
        Self {
            progression,
            scale,
            checker,
            comp,
            bars_per_chord,
            jazziness,
            volume,
            a4,
            shared,
            rng: ChaCha8Rng::seed_from_u64(seed ^ 0x5eed),
            prev_voicing: None,
            anticipated: None,
            skip_downbeat: false,
            last_pos: None,
        }
    }

    /// Voicing for chord number `pos` (counting from the start, not wrapped).
    fn voicing_for(&mut self, pos: usize) -> Vec<u8> {
        if let Some((p, v)) = self.anticipated.take()
            && p == pos
        {
            self.prev_voicing = Some(v.clone());
            return v;
        }
        let chord = &self.progression[pos % self.progression.len()];
        let v = voice(
            chord,
            self.prev_voicing.as_deref(),
            self.jazziness,
            &mut self.rng,
        );
        self.prev_voicing = Some(v.clone());
        v
    }

    fn play(
        &self,
        io: &mut SessionIo,
        shared: &mut ChordShared,
        time: f64,
        len: f64,
        velocity: f32,
        notes: &[u8],
    ) {
        for &midi in notes {
            let kind = SoundKind::Note {
                midi: midi as f32,
                velocity,
                duration: (len * 0.95) as f32,
            };
            io.play(
                time,
                Slot::Instrument,
                SoundEvent {
                    kind,
                    gain: self.volume,
                },
            );
            shared.piano.push(PianoNote { time, len, midi });
        }
    }
}

impl Accompaniment for ChordAccompaniment {
    fn plan_bar(&mut self, bar: &BarPlan, io: &mut SessionIo) {
        if bar.is_count_in() || self.progression.is_empty() {
            return;
        }
        let beats = bar.beats.max(1) as f32;
        let beat = bar.duration / beats as f64;
        let chord_beats = (self.bars_per_chord * beats).max(1.0);
        let bar_beat0 = bar.index as f32 * beats;
        let pos_at = |b: f32| ((bar_beat0 + b) / chord_beats).floor() as usize;
        let time_at = |b: f32| bar.start + b as f64 * beat;

        // Chord spans touching this bar: (start beat in bar, end beat in bar, position).
        let mut segments: Vec<(f32, f32, usize)> = Vec::new();
        let mut b = 0.0;
        while b < beats - 1e-3 {
            let pos = pos_at(b + 1e-3);
            let next_change = ((pos + 1) as f32 * chord_beats - bar_beat0).min(beats);
            segments.push((b, next_change, pos));
            b = next_change;
        }

        // Jazzier players anticipate the next chord by an eighth note: decided up front so
        // this bar's hits can make room for it.
        let next_pos = pos_at(beats + 1e-3);
        let push = beats - 0.5;
        let anticipate =
            next_pos != pos_at(beats - 1e-3) && self.rng.random::<f32>() < self.jazziness * 0.4;
        let cut = if anticipate { push } else { beats };

        let mut shared = lock_chords(&self.shared);
        for &(s, e, pos) in &segments {
            if s == 0.0 && self.last_pos == Some(pos) {
                // The chord is held over the bar line.
                if let Some(last) = shared.spans.last_mut() {
                    last.end = time_at(e);
                }
                continue;
            }
            drop(shared);
            let voicing = self.voicing_for(pos);
            shared = lock_chords(&self.shared);
            let step = pos % self.progression.len();
            shared.spans.push(ChordSpan {
                start: time_at(s),
                end: time_at(e),
                step,
                chord: self.progression[step].clone(),
                voicing,
            });
            self.last_pos = Some(pos);
        }

        // Comp hits, re-attacking chords that change between hits.
        let mut hits = comp::hits(&self.comp, bar.beats);
        if std::mem::take(&mut self.skip_downbeat) {
            hits.retain(|h| h.pos > 0.0);
        }
        for &(s, _, _) in &segments {
            if s > 0.0 && !hits.iter().any(|h| (h.pos - s).abs() < 1e-3) {
                let len = hits
                    .iter()
                    .map(|h| h.pos)
                    .filter(|&p| p > s)
                    .fold(beats, f32::min)
                    - s;
                hits.push(comp::Hit {
                    pos: s,
                    len,
                    velocity: 0.7,
                });
            }
        }
        hits.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        for h in hits.iter().filter(|h| h.pos < cut - 1e-3) {
            let Some(&(_, seg_end, _)) = segments
                .iter()
                .find(|seg| h.pos >= seg.0 - 1e-3 && h.pos < seg.1)
            else {
                continue;
            };
            let Some(span) = shared
                .spans
                .iter()
                .rev()
                .find(|sp| sp.start <= time_at(h.pos) + 1e-6)
            else {
                continue;
            };
            let notes = span.voicing.clone();
            let end = (h.pos + h.len).min(seg_end).min(cut);
            self.play(
                io,
                &mut shared,
                time_at(h.pos),
                (end - h.pos) as f64 * beat,
                h.velocity,
                &notes,
            );
        }

        if anticipate {
            drop(shared);
            let v = self.voicing_for(next_pos);
            self.anticipated = Some((next_pos, v.clone()));
            shared = lock_chords(&self.shared);
            self.play(io, &mut shared, time_at(push), 1.5 * beat, 0.8, &v);
            self.skip_downbeat = true;
        }
    }

    fn judge(&self, time: f64, freq: f32) -> Option<f32> {
        let shared = lock_chords(&self.shared);
        let span = shared
            .spans
            .iter()
            .find(|s| s.start - 1e-6 <= time && time < s.end)?;
        let pc = freq_to_midi(freq, self.a4).0.rem_euclid(12) as u8;
        Some(self.checker.score(pc, &span.chord, &self.scale))
    }

    fn stop(&mut self, now: f64) {
        let mut shared = lock_chords(&self.shared);
        shared.spans.retain(|s| s.start <= now);
        shared.piano.retain(|n| n.time <= now);
    }
}
