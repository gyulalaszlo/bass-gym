//! The simulated keyboard player: steps through the progression, voices each chord with
//! voice leading, comps it with the selected rhythm and judges the bassist's notes.

use std::sync::{Arc, Mutex, MutexGuard};

use bg_audio::{SessionIo, Slot};
use bg_core::harmony::{Chord, ChordChecker, Scale, Voicing, VoicingStyle, comp, voice};
use bg_core::music::freq_to_midi;
use bg_core::plan::{BarPlan, PlannedNote};
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
    /// All notes of the piano voicing, ascending.
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

/// The chords touching bar `index` of `beats` beats: (start beat in the bar, end beat in
/// the bar, progression position counted from the start, not wrapped).
pub fn segments(index: i64, beats: f32, bars_per_chord: f32) -> Vec<(f32, f32, usize)> {
    let chord_beats = (bars_per_chord * beats).max(1.0);
    let bar_beat0 = index.max(0) as f32 * beats;
    let mut out = Vec::new();
    let mut b = 0.0;
    while b < beats - 1e-3 {
        let pos = ((bar_beat0 + b + 1e-3) / chord_beats).floor() as usize;
        let next_change = ((pos + 1) as f32 * chord_beats - bar_beat0).min(beats);
        out.push((b, next_change, pos));
        b = next_change;
    }
    out
}

/// The comp attacks of a bar, sorted: the pattern's hits plus a re-attack wherever the
/// chord changes between hits.
pub fn bar_hits(
    pattern: &str,
    beats: u32,
    segments: &[(f32, f32, usize)],
    skip_downbeat: bool,
) -> Vec<comp::Hit> {
    let end = beats.max(1) as f32;
    let mut hits = comp::hits(pattern, beats);
    if skip_downbeat {
        hits.retain(|h| h.pos > 0.0);
    }
    for &(s, _, _) in segments {
        if s > 0.0 && !hits.iter().any(|h| (h.pos - s).abs() < 1e-3) {
            let len = hits
                .iter()
                .map(|h| h.pos)
                .filter(|&p| p > s)
                .fold(end, f32::min)
                - s;
            hits.push(comp::Hit {
                pos: s,
                len,
                velocity: 0.7,
            });
        }
    }
    hits.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    hits
}

/// A comp hit as a target: from `pos` to `end` (beats in the bar), accented on the
/// downbeat and on chord changes.
fn hit_target(bar: &BarPlan, segments: &[(f32, f32, usize)], pos: f32, end: f32) -> PlannedNote {
    let beat = bar.duration / bar.beats.max(1) as f64;
    PlannedNote {
        time: bar.start + pos as f64 * beat,
        len: (end - pos) as f64 * beat,
        accent: segments.iter().any(|seg| (seg.0 - pos).abs() < 1e-3),
    }
}

/// The comp hits of a bar as targets, without anticipations (they're random), for
/// previewing the targets before a session.
pub fn preview_targets(pattern: &str, bar: &BarPlan, bars_per_chord: f32) -> Vec<PlannedNote> {
    let segs = segments(bar.index, bar.beats.max(1) as f32, bars_per_chord);
    bar_hits(pattern, bar.beats, &segs, false)
        .iter()
        .filter_map(|h| {
            let seg = segs.iter().find(|s| h.pos >= s.0 - 1e-3 && h.pos < s.1)?;
            Some(hit_target(bar, &segs, h.pos, (h.pos + h.len).min(seg.1)))
        })
        .collect()
}

/// Largest humanize changes (at humanize = 1): hit timing (s), hit velocity, chord roll
/// (s) and per-note velocity.
const HUMANIZE_TIMING: f32 = 0.012 * 2.0;
const HUMANIZE_VELOCITY: f32 = 0.15 * 2.0;
const HUMANIZE_ROLL: f32 = 0.010 * 4.0;
const HUMANIZE_NOTE_VELOCITY: f32 = 0.05 * 4.0;

pub struct ChordAccompaniment {
    pub progression: Vec<Chord>,
    pub scale: Scale,
    pub checker: &'static dyn ChordChecker,
    pub comp: String,
    /// Chord length as a multiple of the bar (0.5, 1, 2, 4).
    pub bars_per_chord: f32,
    pub style: VoicingStyle,
    /// 0 = machine-exact … 1 = loose: random timing, rolled chords and velocity changes.
    pub humanize: f32,
    pub volume: f32,
    pub a4: f32,
    pub shared: SharedChords,
    rng: ChaCha8Rng,
    /// Randomness of the humanized feel (separate, so voicings don't depend on it).
    feel_rng: ChaCha8Rng,
    prev_voicing: Option<Voicing>,
    /// Voicing prepared early for an anticipated chord (progression position, voicing).
    anticipated: Option<(usize, Voicing)>,
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
        style: VoicingStyle,
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
            style,
            humanize: 0.0,
            volume,
            a4,
            shared,
            rng: ChaCha8Rng::seed_from_u64(seed ^ 0x5eed),
            feel_rng: ChaCha8Rng::seed_from_u64(seed ^ 0xfee1),
            prev_voicing: None,
            anticipated: None,
            skip_downbeat: false,
            last_pos: None,
        }
    }

    /// A humanized player (see [`ChordAccompaniment::humanize`]).
    pub fn with_humanize(mut self, humanize: f32) -> Self {
        self.humanize = humanize.clamp(0.0, 1.0);
        self
    }

    /// Voicing for chord number `pos` (counting from the start, not wrapped).
    fn voicing_for(&mut self, pos: usize) -> Voicing {
        if let Some((p, v)) = self.anticipated.take()
            && p == pos
        {
            self.prev_voicing = Some(v.clone());
            return v;
        }
        let chord = &self.progression[pos % self.progression.len()];
        let v = voice(
            chord,
            self.prev_voicing.as_ref(),
            &self.style,
            &mut self.rng,
        );
        self.prev_voicing = Some(v.clone());
        v
    }

    /// Plays `notes` at `time` for `len` seconds, humanized by `humanize`, and records
    /// them for the UI.
    #[allow(clippy::too_many_arguments)]
    fn play(
        rng: &mut ChaCha8Rng,
        humanize: f32,
        volume: f32,
        io: &mut SessionIo,
        shared: &mut ChordShared,
        time: f64,
        len: f64,
        velocity: f32,
        notes: &[u8],
    ) {
        let h = humanize.clamp(0.0, 1.0);
        let mut jitter = |amount: f32| {
            if h > 0.0 {
                rng.random_range(-1.0f32..=1.0) * amount * h
            } else {
                0.0
            }
        };
        // Per hit: timing push / drag and dynamics; per note: a roll from the bottom and
        // a little velocity variation.
        let hit_dt = jitter(HUMANIZE_TIMING) as f64;
        let hit_dv = jitter(HUMANIZE_VELOCITY);
        let roll = jitter(HUMANIZE_ROLL).abs() as f64;
        let end = time + len;
        for (k, &midi) in notes.iter().enumerate() {
            let spread = if notes.len() > 1 {
                roll * k as f64 / (notes.len() - 1) as f64
            } else {
                0.0
            };
            let t = (time + hit_dt + spread).max(io.now + 0.005);
            let len = (end - t).max(0.05);
            let velocity = (velocity + hit_dv + jitter(HUMANIZE_NOTE_VELOCITY)).clamp(0.05, 1.0);
            let kind = SoundKind::Note {
                midi: midi as f32,
                velocity,
                duration: (len * 0.95) as f32,
            };
            io.play(t, Slot::Instrument, SoundEvent { kind, gain: volume });
            shared.piano.push(PianoNote { time: t, len, midi });
        }
    }
}

impl Accompaniment for ChordAccompaniment {
    fn plan_bar(&mut self, bar: &BarPlan, io: &mut SessionIo) -> Vec<PlannedNote> {
        if bar.is_count_in() || self.progression.is_empty() {
            return Vec::new();
        }
        let beats = bar.beats.max(1) as f32;
        let beat = bar.duration / beats as f64;
        let time_at = |b: f32| bar.start + b as f64 * beat;
        // Jazzier players anticipate the next chord by an eighth note: decided up front so
        // this bar's hits can make room for it.
        let next_pos = segments(bar.index + 1, beats, self.bars_per_chord)[0].2;
        let segments = segments(bar.index, beats, self.bars_per_chord);
        let push = beats - 0.5;
        let anticipate = segments.last().is_some_and(|seg| seg.2 != next_pos)
            && self.rng.random::<f32>() < self.style.jazziness * 0.4;
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
                voicing: voicing.notes(),
            });
            self.last_pos = Some(pos);
        }

        // Comp hits, re-attacking chords that change between hits.
        let skip = std::mem::take(&mut self.skip_downbeat);
        let hits = bar_hits(&self.comp, bar.beats, &segments, skip);
        let mut targets = Vec::new();
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
            Self::play(
                &mut self.feel_rng,
                self.humanize,
                self.volume,
                io,
                &mut shared,
                time_at(h.pos),
                (end - h.pos) as f64 * beat,
                h.velocity,
                &notes,
            );
            targets.push(hit_target(bar, &segments, h.pos, end));
        }

        if anticipate {
            drop(shared);
            let v = self.voicing_for(next_pos);
            self.anticipated = Some((next_pos, v.clone()));
            shared = lock_chords(&self.shared);
            Self::play(
                &mut self.feel_rng,
                self.humanize,
                self.volume,
                io,
                &mut shared,
                time_at(push),
                1.5 * beat,
                0.8,
                &v.notes(),
            );
            self.skip_downbeat = true;
            targets.push(PlannedNote {
                time: time_at(push),
                len: 0.5 * beat,
                accent: true,
            });
        }
        targets
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
