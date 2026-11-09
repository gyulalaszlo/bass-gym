//! Note onset / offset detection on a monophonic bass signal.
//!
//! Works on short hops (~0.7 ms). Three onset cues are combined:
//! - the signal peak rising above the highest peak of the preceding ~40 ms (which spans a
//!   full period of a ringing low note, so its waveform ripple doesn't trigger),
//! - a dip and rise of the note's level, measured over one period of the note itself. A
//!   mute before the next note can be far shorter than those ~40 ms, as it is at high
//!   tempos, and then the first cue never sees the level fall, and
//! - a burst in the high band (> 500 Hz): the pluck transient. Its short reference window
//!   catches re-plucks after mutes shorter than one period of a low note. It must also
//!   beat the bursts of the last full period, as a low note's waveform can have a sharp
//!   edge in every period. It's only a candidate until the low band confirms a rise
//!   (muting a string also makes a click).
//!
//! The note ends when its short-term RMS falls a set amount below its peak, or when the
//! next onset arrives.
//!
//! Each onset is then held for [`HOLD_MS`] before it's reported, to look at what follows:
//! - String noise (a finger touching or leaving a string) stays far below the notes
//!   played before it; it's reported as [`NoteEvent::Noise`], not as a note.
//! - A muted pluck (ghost note) often starts with the faint sound of the finger meeting
//!   the string, its thud following 15-30 ms later. When the level jumps like that, the
//!   onset moves to the jump.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::filter::Biquad;
use crate::{db, from_db};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteDetectorConfig {
    /// Signals below this level are ignored.
    pub gate_db: f32,
    /// Required envelope rise over its recent history to count as a new note.
    pub onset_rise_db: f32,
    pub min_gap_ms: f32,
    /// A note ends when its RMS falls this far below its peak RMS. Lower values end
    /// notes earlier (suits soft, gradual muting); higher values wait for a deeper drop.
    pub release_db: f32,
    /// Shifts every detected note end by this much (negative = earlier), to compensate
    /// for how quickly or slowly notes are muted.
    pub end_offset_ms: f32,
    /// A note whose peak stays this far below the recent notes' is string noise; 0 keeps
    /// everything.
    pub noise_db: f32,
}

impl Default for NoteDetectorConfig {
    fn default() -> Self {
        Self {
            gate_db: -45.0,
            onset_rise_db: 5.0,
            min_gap_ms: 45.0,
            release_db: 24.0,
            end_offset_ms: 0.0,
            noise_db: 12.0,
        }
    }
}

/// How far a high band burst must rise above those of the last period of a low note.
const HF_PERIOD_RISE_DB: f32 = 1.0;
/// Periods looked for. Higher notes are found at a multiple of their period, which is
/// just as good here, and the longest period is that of a low B.
const PERIOD_MIN_MS: f32 = 5.0;
const PERIOD_MAX_MS: f32 = 34.0;
/// How much of the signal is compared with itself one period earlier.
const PERIOD_WINDOW_MS: f32 = 24.0;
/// How alike those must be to count as a steady note.
const PERIOD_MIN_CORRELATION: f32 = 0.8;
const PERIOD_EVERY_MS: f32 = 4.0;
/// A smaller change of the measured period is ignored.
const PERIOD_TOLERANCE: f32 = 0.1;
/// Slack for that, when measuring a level over a period.
const PERIOD_MARGIN: f32 = 1.15;
/// A rise out of a dip must take no longer than this. A pluck is that fast; two strings
/// beating against each other are not.
const DIP_RISE_MS: f32 = 25.0;
/// A rise out of a dip must come back to within this of the note's peak. Anything lower
/// is the noise of the mute itself.
const DIP_RETURN_DB: f32 = 15.0;

/// How long an onset is held before it's reported, to see what follows it.
pub const HOLD_MS: f32 = 40.0;
/// A level this far above everything since the onset, bar the last [`JUMP_RISE_MS`],
/// means the onset was a precursor and the note begins with that rise.
const JUMP_DB: f32 = 12.0;
/// Longer than a pluck's own attack, which on a low note takes several milliseconds to
/// reach its first full peak.
const JUMP_RISE_MS: f32 = 8.0;
/// The precursor must last at least this long: a pluck's own first moments are low too.
const PRECURSOR_MS: f32 = 5.0;
/// Notes whose levels make up the reference for telling string noise apart.
const REFERENCE_NOTES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteEvent {
    /// `sample` counts from the first sample ever passed to the detector. Reported
    /// [`HOLD_MS`] after the onset.
    On {
        sample: u64,
        level_db: f32,
    },
    Off {
        sample: u64,
        peak_db: f32,
    },
    /// String noise from `sample` to `end`: no note was played.
    Noise {
        sample: u64,
        end: u64,
        peak_db: f32,
    },
}

/// An onset not reported yet (see [`HOLD_MS`]).
#[derive(Clone, Copy, Debug)]
struct Held {
    sample: u64,
    /// Start of the hop the onset was found in: `held_peaks` counts hops from here.
    first_hop: u64,
    level: f32,
    /// Highest peak so far.
    peak: f32,
    /// Where the level jumped, if it did.
    jump: Option<u64>,
    /// The note already ended: (sample, peak).
    off: Option<(u64, f32)>,
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    sample: u64,
    floor: f32,
    hops_left: u32,
}

#[derive(Clone, Copy, Debug)]
struct Active {
    peak: f32,
    peak_rms: f32,
}

/// Measures the period of the ringing note, in hops, by comparing the signal with itself.
/// An error can only be a multiple of the period, since nothing matches better than the
/// period itself, and that only makes a dip harder to see.
struct PeriodTracker {
    hist: VecDeque<f32>,
    min_lag: usize,
    max_lag: usize,
    window: usize,
    every: usize,
    wait: usize,
    period: Option<usize>,
}

impl PeriodTracker {
    fn new(hops: impl Fn(f32) -> usize) -> Self {
        Self {
            hist: VecDeque::new(),
            min_lag: hops(PERIOD_MIN_MS),
            max_lag: hops(PERIOD_MAX_MS),
            window: hops(PERIOD_WINDOW_MS),
            every: hops(PERIOD_EVERY_MS),
            wait: 0,
            period: None,
        }
    }

    /// Forgets the note. The next one can be much lower, and with this one's period its
    /// ripple would look like dips.
    fn reset(&mut self) {
        self.hist.clear();
        self.period = None;
    }

    fn push(&mut self, sample: f32) {
        self.hist.push_back(sample);
        if self.hist.len() > self.max_lag + self.window {
            self.hist.pop_front();
            self.wait = self.wait.saturating_sub(1);
            if self.wait == 0 {
                self.wait = self.every;
                self.measure();
            }
        }
    }

    /// Keeps the last period when the signal isn't steady, as during a mute.
    fn measure(&mut self) {
        let hist = self.hist.make_contiguous();
        let now = &hist[hist.len() - self.window..];
        let energy = |s: &[f32]| s.iter().map(|v| v * v).sum::<f32>();
        let now_energy = energy(now);
        let mut best = (PERIOD_MIN_CORRELATION, None);
        for lag in self.min_lag..=self.max_lag {
            let start = hist.len() - self.window - lag;
            let then = &hist[start..start + self.window];
            let dot: f32 = now.iter().zip(then).map(|(a, b)| a * b).sum();
            let correlation = dot / (now_energy * energy(then)).sqrt().max(f32::MIN_POSITIVE);
            if correlation > best.0 {
                best = (correlation, Some(lag));
            }
        }
        let Some(lag) = best.1 else {
            return;
        };
        let moved = |p: usize| p.abs_diff(lag) as f32 > p as f32 * PERIOD_TOLERANCE;
        if self.period.is_none_or(moved) {
            self.period = Some(lag);
        }
    }
}

pub struct NoteDetector {
    cfg: NoteDetectorConfig,
    hpf: Biquad,
    hf: [Biquad; 2],
    hop: usize,
    hop_buf: Vec<f32>,
    hf_buf: Vec<f32>,
    hf_hist: VecDeque<f32>,
    hf_hist_len: usize,
    /// Hops of `hf_hist` that make up the short reference window.
    hf_short_len: usize,
    hf_skip: usize,
    candidate: Option<Candidate>,
    confirm_hops: u32,
    floor_hops: usize,
    next_sample: u64,
    env: f32,
    release: f32,
    peak_hist: VecDeque<f32>,
    hist_len: usize,
    hist_skip: usize,
    period: PeriodTracker,
    /// The note's level over one period, for as long as that period holds.
    level_hist: VecDeque<f32>,
    level_hist_len: usize,
    level_span: usize,
    energy_hist: VecDeque<f32>,
    rms_hops: usize,
    active: Option<Active>,
    last_onset: Option<u64>,
    held: Option<Held>,
    /// Hop peaks since the held onset.
    held_peaks: Vec<f32>,
    /// String noise still sounding: its start and peak. Its end becomes the noise's end.
    noise: Option<(u64, f32)>,
    /// Peaks of the last notes, for telling string noise apart.
    recent_peaks: VecDeque<f32>,
    raw: Vec<NoteEvent>,
    sr: f32,
}

impl NoteDetector {
    pub fn new(sr: f32, cfg: NoteDetectorConfig) -> Self {
        let hop = ((sr / 1500.0) as usize).max(8);
        let hop_secs = hop as f32 / sr;
        let hops = |ms: f32| ((ms / 1000.0 / hop_secs).round() as usize).max(1);
        Self {
            cfg,
            hpf: Biquad::highpass(sr, 20.0, 0.707),
            hf: [
                Biquad::highpass(sr, 500.0, 0.54),
                Biquad::highpass(sr, 500.0, 1.31),
            ],
            hop,
            hop_buf: Vec::with_capacity(hop),
            hf_buf: Vec::with_capacity(hop),
            hf_hist: VecDeque::new(),
            hf_hist_len: hops(42.0),
            hf_short_len: hops(15.0),
            hf_skip: hops(3.0),
            candidate: None,
            confirm_hops: hops(15.0) as u32,
            floor_hops: hops(5.0),
            next_sample: 0,
            env: 0.0,
            release: (-hop_secs / 0.060).exp(),
            peak_hist: VecDeque::new(),
            hist_len: hops(42.0),
            hist_skip: hops(6.0),
            period: PeriodTracker::new(hops),
            level_hist: VecDeque::new(),
            level_hist_len: hops(DIP_RISE_MS),
            level_span: 0,
            energy_hist: VecDeque::new(),
            rms_hops: hops(40.0),
            active: None,
            last_onset: None,
            held: None,
            held_peaks: Vec::new(),
            noise: None,
            recent_peaks: VecDeque::new(),
            raw: Vec::new(),
            sr,
        }
    }

    pub fn set_config(&mut self, cfg: NoteDetectorConfig) {
        self.cfg = cfg;
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    pub fn is_note_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn process(&mut self, samples: &[f32], out: &mut Vec<NoteEvent>) {
        for &x in samples {
            self.hop_buf.push(self.hpf.process(x));
            let h = self.hf[0].process(x);
            self.hf_buf.push(self.hf[1].process(h));
            if self.hop_buf.len() == self.hop {
                let mut raw = std::mem::take(&mut self.raw);
                raw.clear();
                self.process_hop(&mut raw);
                self.settle(&raw, out);
                self.raw = raw;
                self.next_sample += self.hop as u64;
                self.hop_buf.clear();
                self.hf_buf.clear();
            }
        }
    }

    fn emit_onset(&mut self, sample: u64, level: f32, out: &mut Vec<NoteEvent>) {
        if let Some(a) = self.active.take() {
            out.push(NoteEvent::Off {
                sample,
                peak_db: db(a.peak),
            });
        }
        out.push(NoteEvent::On {
            sample,
            level_db: db(level),
        });
        self.active = Some(Active {
            peak: level,
            peak_rms: 0.0,
        });
        self.last_onset = Some(sample);
        self.period.reset();
        self.level_hist.clear();
    }

    /// Passes on what this hop found, holding onsets back for [`HOLD_MS`]: see the
    /// module docs.
    fn settle(&mut self, raw: &[NoteEvent], out: &mut Vec<NoteEvent>) {
        let hop_start = self.next_sample;
        let hop_peak = self.hop_buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        for &ev in raw {
            match ev {
                NoteEvent::On { sample, level_db } => {
                    self.release_held(out);
                    if let Some((start, peak)) = self.noise.take() {
                        out.push(NoteEvent::Noise {
                            sample: start,
                            end: sample,
                            peak_db: db(peak),
                        });
                    }
                    self.held_peaks.clear();
                    self.held = Some(Held {
                        sample,
                        first_hop: self.next_sample,
                        level: from_db(level_db),
                        peak: 0.0,
                        jump: None,
                        off: None,
                    });
                }
                NoteEvent::Off { sample, peak_db } => {
                    if let Some(h) = &mut self.held {
                        h.off = Some((sample, peak_db));
                    } else if let Some((start, peak)) = self.noise.take() {
                        out.push(NoteEvent::Noise {
                            sample: start,
                            end: sample,
                            peak_db: db(peak),
                        });
                    } else {
                        out.push(ev);
                    }
                }
                NoteEvent::Noise { .. } => out.push(ev),
            }
        }
        let Some(h) = &mut self.held else {
            return;
        };
        let since = |ms: f32| (ms / 1000.0 * self.sr) as u64;
        if h.off.is_none() {
            let skip = (since(JUMP_RISE_MS) as usize).div_ceil(self.hop);
            let precursor = (since(PRECURSOR_MS) as usize).div_ceil(self.hop);
            let peaks = &self.held_peaks;
            if h.jump.is_none() && peaks.len() >= skip + precursor {
                let before = peaks[..peaks.len() - skip]
                    .iter()
                    .fold(0.0f32, |m, &v| m.max(v));
                if hop_peak > before * from_db(JUMP_DB) {
                    // Back to where the rise left the precursor's level.
                    let clear = before * from_db(JUMP_DB / 2.0);
                    let k = peaks.iter().rposition(|&v| v <= clear).map_or(0, |i| i + 1);
                    h.jump = Some(if k < peaks.len() {
                        h.first_hop + (k * self.hop) as u64
                    } else {
                        let idx = self
                            .hop_buf
                            .iter()
                            .position(|v| v.abs() >= clear)
                            .unwrap_or(0);
                        hop_start + idx as u64
                    });
                }
            }
            self.held_peaks.push(hop_peak);
            h.peak = h.peak.max(hop_peak);
        }
        if hop_start + self.hop as u64 >= h.sample + since(HOLD_MS) {
            self.release_held(out);
        }
    }

    /// Reports the held onset: as a note, or as string noise.
    fn release_held(&mut self, out: &mut Vec<NoteEvent>) {
        let Some(h) = self.held.take() else {
            return;
        };
        let peak = h.peak.max(h.level);
        let mut levels: Vec<f32> = self.recent_peaks.iter().copied().collect();
        levels.sort_by(f32::total_cmp);
        let reference = levels.get(levels.len() / 2).copied();
        let noise = self.cfg.noise_db > 0.0
            && reference.is_some_and(|r| peak < r * from_db(-self.cfg.noise_db));
        if noise {
            match h.off {
                Some((end, _)) => out.push(NoteEvent::Noise {
                    sample: h.sample,
                    end,
                    peak_db: db(peak),
                }),
                None => self.noise = Some((h.sample, peak)),
            }
            return;
        }
        self.recent_peaks.push_back(peak);
        if self.recent_peaks.len() > REFERENCE_NOTES {
            self.recent_peaks.pop_front();
        }
        let sample = h.jump.unwrap_or(h.sample);
        out.push(NoteEvent::On {
            sample,
            level_db: db(h.level),
        });
        if let Some((end, peak_db)) = h.off {
            out.push(NoteEvent::Off {
                sample: end.max(sample + 1),
                peak_db,
            });
        }
    }

    /// Where the note's level rose out of a dip, if it just did. The level is the highest
    /// peak of one period, so it's free of ripple and still shows a mute of a few periods.
    fn dip_onset(&mut self, hop_peak: f32, hop_start: u64) -> Option<u64> {
        let period = self.period.period?;
        let span = (period as f32 * PERIOD_MARGIN).ceil() as usize;
        if span != self.level_span {
            // Levels over different spans don't compare: a longer one reaching back to
            // louder peaks would look like a rise.
            self.level_span = span;
            self.level_hist.clear();
        }
        let level = self
            .peak_hist
            .iter()
            .rev()
            .take(span - 1)
            .fold(hop_peak, |m, &v| m.max(v));
        self.level_hist.push_back(level);
        if self.level_hist.len() > self.level_hist_len {
            self.level_hist.pop_front();
        }
        let (trough_at, trough) =
            self.level_hist
                .iter()
                .enumerate()
                .fold(
                    (0, f32::MAX),
                    |low, (i, &v)| {
                        if v <= low.1 { (i, v) } else { low }
                    },
                );
        let returned = self
            .active
            .is_some_and(|a| level > a.peak * from_db(-DIP_RETURN_DB));
        let risen = returned
            && level > from_db(self.cfg.gate_db)
            && level > trough * from_db(self.cfg.onset_rise_db);
        risen.then(|| {
            // Halfway up the rise: the noise of a mute wobbles just above its trough.
            let clear = trough * from_db(self.cfg.onset_rise_db / 2.0);
            let hops_ago = self
                .peak_hist
                .iter()
                .rev()
                .take(self.level_hist.len() - trough_at - 1)
                .enumerate()
                .rfind(|(_, v)| **v > clear)
                .map_or(0, |(i, _)| i + 1);
            hop_start - (hops_ago * self.hop) as u64
        })
    }

    fn process_hop(&mut self, out: &mut Vec<NoteEvent>) {
        let hop_start = self.next_sample;
        let hop_peak = self.hop_buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let energy: f32 = self.hop_buf.iter().map(|v| v * v).sum();
        self.env = hop_peak.max(self.env * self.release);
        let mean = self.hop_buf.iter().sum::<f32>() / self.hop as f32;
        self.period.push(mean);

        // Peaks of the recent past (long enough to span a full period of the lowest note)
        // vs. the peak of the last couple of hops.
        let n = self.peak_hist.len();
        let reference = self
            .peak_hist
            .iter()
            .take(n.saturating_sub(self.hist_skip))
            .fold(0.0f32, |m, &v| m.max(v));
        let current = self.peak_hist.back().copied().unwrap_or(0.0).max(hop_peak);
        let gate = from_db(self.cfg.gate_db);
        let min_gap = (self.cfg.min_gap_ms / 1000.0 * self.sr) as u64;

        let hf_peak = self.hf_buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let n = self.hf_hist.len();
        let older = n.saturating_sub(self.hf_skip);
        let short_start = n.saturating_sub(self.hf_short_len);
        let highest = |range: std::ops::Range<usize>| {
            self.hf_hist.range(range).fold(0.0f32, |m, &v| m.max(v))
        };
        let hf_ref = highest(short_start.min(older)..older);
        let hf_period = highest(0..older);
        let hf_onset = hf_peak > gate * 0.25
            && hf_peak > hf_ref * from_db(self.cfg.onset_rise_db + 6.0)
            && hf_peak > hf_period * from_db(HF_PERIOD_RISE_DB)
            && n >= self.hf_short_len;
        let lf_onset = current > reference * from_db(self.cfg.onset_rise_db);
        self.hf_hist.push_back(hf_peak);
        if self.hf_hist.len() > self.hf_hist_len {
            self.hf_hist.pop_front();
        }

        let rise = from_db(self.cfg.onset_rise_db);
        let floor = self
            .peak_hist
            .iter()
            .rev()
            .take(self.floor_hops)
            .fold(0.0f32, |m, &v| m.max(v));
        if let Some(c) = &mut self.candidate {
            if hop_peak > gate && hop_peak > c.floor * rise {
                let (sample, level) = (c.sample, hop_peak);
                self.candidate = None;
                self.emit_onset(sample, level, out);
            } else {
                c.hops_left = c.hops_left.saturating_sub(1);
                if c.hops_left == 0 {
                    self.candidate = None;
                }
            }
        }
        let gap_ok = self.last_onset.is_none_or(|t| hop_start >= t + min_gap);
        let dip = self.dip_onset(hop_peak, hop_start);
        if current > gate && lf_onset && gap_ok {
            let threshold = (hop_peak * 0.3).max(gate * 0.5);
            let idx = self
                .hop_buf
                .iter()
                .position(|v| v.abs() >= threshold)
                .unwrap_or(0);
            self.candidate = None;
            self.emit_onset(hop_start + idx as u64, hop_peak, out);
        } else if let Some(sample) = dip.filter(|_| gap_ok) {
            self.candidate = None;
            self.emit_onset(sample, hop_peak, out);
        } else if hf_onset && gap_ok && self.candidate.is_none() {
            let idx = self
                .hf_buf
                .iter()
                .position(|v| v.abs() >= hf_peak * 0.3)
                .unwrap_or(0);
            self.candidate = Some(Candidate {
                sample: hop_start + idx as u64,
                floor,
                hops_left: self.confirm_hops,
            });
        }

        self.peak_hist.push_back(hop_peak);
        if self.peak_hist.len() > self.hist_len {
            self.peak_hist.pop_front();
        }
        self.energy_hist.push_back(energy);
        if self.energy_hist.len() > self.rms_hops {
            self.energy_hist.pop_front();
        }
        let rms = (self.energy_hist.iter().sum::<f32>()
            / (self.energy_hist.len() * self.hop) as f32)
            .sqrt();

        if let Some(a) = &mut self.active {
            a.peak = a.peak.max(hop_peak);
            a.peak_rms = a.peak_rms.max(rms);
            let settled = self
                .last_onset
                .is_some_and(|t| hop_start > t + (self.rms_hops * self.hop) as u64);
            if settled
                && (rms < a.peak_rms * from_db(-self.cfg.release_db) || self.env < gate * 0.5)
            {
                // With a release threshold this far down, the RMS window is almost entirely
                // past the end of the note when it triggers; place the end at its start.
                let end = (hop_start + self.hop as u64) as i64 - (self.rms_hops * self.hop) as i64
                    + (self.cfg.end_offset_ms / 1000.0 * self.sr) as i64;
                let onset = self.last_onset.unwrap_or(0) as i64 + 1;
                let sample = end.max(onset) as u64;
                out.push(NoteEvent::Off {
                    sample,
                    peak_db: db(a.peak),
                });
                self.active = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    /// Renders plucks: (start s, duration s, freq, amplitude). Notes are damped at the end.
    fn render(sr: f32, secs: f32, notes: &[(f32, f32, f32, f32)]) -> Vec<f32> {
        let mut out = vec![0.0f32; (sr * secs) as usize];
        let mut seed = 7u32;
        for &(start, dur, freq, amp) in notes {
            let s0 = (start * sr) as usize;
            let len = ((dur + 0.03) * sr) as usize;
            for i in 0..len.min(out.len().saturating_sub(s0)) {
                let t = i as f32 / sr;
                let attack = (t / 0.002).min(1.0);
                let decay = (-t / 1.2).exp();
                let damp = if t > dur {
                    (-(t - dur) / 0.006).exp()
                } else {
                    1.0
                };
                let tone: f32 = (1..6)
                    .map(|h| (TAU * freq * h as f32 * t).sin() / h as f32)
                    .sum();
                // Finger / pick transient: a short broadband click.
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let click =
                    ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * (-t / 0.0015).exp() * 0.3;
                out[s0 + i] += amp * (attack * decay * damp * tone * 0.5 + click);
            }
        }
        out
    }

    fn detect(sr: f32, signal: &[f32]) -> Vec<NoteEvent> {
        let mut d = NoteDetector::new(sr, NoteDetectorConfig::default());
        let mut ev = Vec::new();
        for chunk in signal.chunks(256) {
            d.process(chunk, &mut ev);
        }
        ev
    }

    #[test]
    fn onsets_are_precise() {
        let sr = 48_000.0;
        let notes = [
            (0.25, 0.4, 41.2, 0.5),
            (0.75, 0.2, 55.0, 0.3),
            (1.0, 0.45, 61.7, 0.6),
            (1.5, 0.3, 30.87, 0.4),
        ];
        let ev = detect(sr, &render(sr, 2.5, &notes));
        let ons: Vec<f32> = ev
            .iter()
            .filter_map(|e| {
                if let NoteEvent::On { sample, .. } = e {
                    Some(*sample as f32 / sr)
                } else {
                    None
                }
            })
            .collect();
        let offs: Vec<f32> = ev
            .iter()
            .filter_map(|e| {
                if let NoteEvent::Off { sample, .. } = e {
                    Some(*sample as f32 / sr)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(ons.len(), notes.len(), "{ev:?}");
        assert_eq!(offs.len(), notes.len(), "{ev:?}");
        for ((on, off), n) in ons.iter().zip(&offs).zip(&notes) {
            assert!((on - n.0).abs() < 0.003, "onset {on} vs {}", n.0);
            let end = n.0 + n.1;
            assert!((off - end).abs() < 0.015, "offset {off} vs {end}");
        }
    }

    #[test]
    fn ringing_note_triggers_once_and_legato_splits() {
        let sr = 44_100.0;
        let ev = detect(sr, &render(sr, 3.0, &[(0.1, 2.5, 30.87, 0.8)]));
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, NoteEvent::On { .. }))
                .count(),
            1,
            "{ev:?}"
        );
        // Legato: second note starts while the first still rings.
        let mut sig = render(sr, 1.5, &[(0.1, 1.2, 41.2, 0.3)]);
        let second = render(sr, 1.5, &[(0.6, 0.6, 55.0, 0.6)]);
        sig.iter_mut().zip(&second).for_each(|(a, b)| *a += b);
        let ev = detect(sr, &sig);
        let ons: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e, NoteEvent::On { .. }))
            .collect();
        assert_eq!(ons.len(), 2, "{ev:?}");
    }

    #[test]
    fn repeated_note_after_short_mute() {
        // Same pitch and level, the previous note muted only ~20 ms before the next one.
        let sr = 48_000.0;
        let notes = [
            (0.25, 0.23, 41.2, 0.5),
            (0.5, 0.23, 41.2, 0.5),
            (0.75, 0.23, 41.2, 0.5),
        ];
        let ev = detect(sr, &render(sr, 1.5, &notes));
        let ons: Vec<f32> = ev
            .iter()
            .filter_map(|e| {
                if let NoteEvent::On { sample, .. } = e {
                    Some(*sample as f32 / sr)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(ons.len(), 3, "{ev:?}");
        for (on, n) in ons.iter().zip(&notes) {
            assert!((on - n.0).abs() < 0.003, "onset {on} vs {}", n.0);
        }
    }

    #[test]
    fn sharp_edges_of_a_ringing_note_are_not_plucks() {
        // A low note with an edge in every period, like a pulse wave: the edges are far
        // above the quiet part of the cycle, but no higher than the period before.
        let sr = 48_000.0;
        let period = (sr / 41.2) as usize;
        let mut sig = vec![0.0f32; sr as usize];
        for (i, s) in sig.iter_mut().enumerate().skip(4800) {
            let t = (i - 4800) as f32 / sr;
            let phase = i % period;
            let edge = if phase < 8 { 1.0 } else { 0.0 };
            let body = (TAU * 41.2 * t).sin();
            *s = (-t / 1.5).exp() * 0.4 * (body + edge);
        }
        let ons = detect(sr, &sig)
            .iter()
            .filter(|e| matches!(e, NoteEvent::On { .. }))
            .count();
        assert_eq!(ons, 1);
    }

    /// Onset times of `signal`, in seconds.
    fn onsets(sr: f32, signal: &[f32]) -> Vec<f32> {
        detect(sr, signal)
            .iter()
            .filter_map(|e| match e {
                NoteEvent::On { sample, .. } => Some(*sample as f32 / sr),
                _ => None,
            })
            .collect()
    }

    /// One pitch whose level follows `level`, at `t` seconds. Harmonics stay in phase, so
    /// nothing but the level tells notes apart, and there's no pluck click.
    fn shaped(sr: f32, secs: f32, freq: f32, level: impl Fn(f32) -> f32) -> Vec<f32> {
        (0..(sr * secs) as usize)
            .map(|i| {
                let t = i as f32 / sr;
                let tone: f32 = (1..5)
                    .map(|h| (TAU * freq * h as f32 * t).sin() / h as f32)
                    .sum();
                level(t) * tone * 0.3
            })
            .collect()
    }

    /// Level of notes at `starts`, each muted to -12 dB for the `mute` seconds before the
    /// next one, and rising back over 8 ms.
    fn muted_notes(t: f32, starts: &[f32], mute: f32) -> f32 {
        let Some(&next) = starts.iter().find(|&&s| s > t) else {
            return 1.0;
        };
        if t < starts[0] {
            0.0
        } else if t > next - mute {
            0.25
        } else {
            let since = t - starts.iter().rev().find(|&&s| s <= t).unwrap_or(&t);
            0.25 + 0.75 * (since / 0.008).min(1.0)
        }
    }

    #[test]
    fn a_mute_shorter_than_a_low_period_window_separates_notes() {
        // Eighths at 140 BPM on a D, muted for 30 ms: within the ~40 ms that spans a period
        // of the lowest note, but several periods of this one. With no pluck in this signal
        // a rise only shows at the next waveform peak, so onsets can be a period late.
        let sr = 44_100.0;
        let starts: Vec<f32> = (0..6).map(|i| 0.2 + i as f32 * 0.214).collect();
        let sig = shaped(sr, 1.5, 73.4, |t| muted_notes(t, &starts, 0.03));
        let found = onsets(sr, &sig);
        assert_eq!(found.len(), starts.len(), "{found:?}");
        for (f, s) in found.iter().zip(&starts) {
            assert!((f - s).abs() < 0.014, "{found:?}");
        }
    }

    #[test]
    fn slow_beating_is_not_a_new_note() {
        // Two strings beating at 4 Hz, swinging the level by 12 dB. A much deeper swing
        // falls and rises as fast as a mute, and can't be told from one by level alone.
        let sr = 44_100.0;
        let sig = shaped(sr, 2.0, 73.4, |t| {
            if t < 0.1 {
                0.0
            } else {
                0.625 + 0.375 * (TAU * 4.0 * t).cos()
            }
        });
        assert_eq!(onsets(sr, &sig).len(), 1);
    }

    #[test]
    fn a_low_note_after_a_high_one_is_not_split_by_its_ripple() {
        // The low note has a strong octave, so its peaks alternate in height, and the high
        // note's shorter period must not carry over to it.
        let sr = 44_100.0;
        let mut sig = render(sr, 1.5, &[(0.1, 0.3, 196.0, 0.5)]);
        for (i, s) in sig.iter_mut().enumerate().skip((0.45 * sr) as usize) {
            let t = i as f32 / sr - 0.45;
            let f = 41.2;
            let tone = (TAU * f * t).sin() * 0.4 + (TAU * 2.0 * f * t).sin();
            *s += (t / 0.002).min(1.0) * (-t / 1.2).exp() * 0.4 * tone;
        }
        let found = onsets(sr, &sig);
        assert_eq!(found.len(), 2, "{found:?}");
    }

    #[test]
    fn end_offset_shifts_note_ends() {
        let sr = 48_000.0;
        let sig = render(sr, 1.0, &[(0.2, 0.3, 55.0, 0.5)]);
        let end = |offset_ms: f32| {
            let mut d = NoteDetector::new(
                sr,
                NoteDetectorConfig {
                    end_offset_ms: offset_ms,
                    ..Default::default()
                },
            );
            let mut ev = Vec::new();
            d.process(&sig, &mut ev);
            ev.iter()
                .find_map(|e| {
                    if let NoteEvent::Off { sample, .. } = e {
                        Some(*sample as f32 / sr)
                    } else {
                        None
                    }
                })
                .unwrap()
        };
        assert!((end(-20.0) - (end(0.0) - 0.020)).abs() < 0.001);
        assert!((end(15.0) - (end(0.0) + 0.015)).abs() < 0.001);
    }

    #[test]
    fn quiet_noise_is_ignored() {
        let sr = 48_000.0;
        let mut seed = 1u32;
        let noise: Vec<f32> = (0..48_000)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * 0.004
            })
            .collect();
        assert!(detect(sr, &noise).is_empty());
    }
}
