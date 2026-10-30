//! Note onset / offset detection on a monophonic bass signal.
//!
//! Works on short hops (~0.7 ms). Two onset cues are combined:
//! - the signal peak rising above the highest peak of the preceding ~40 ms (which spans a
//!   full period of a ringing low note, so its waveform ripple doesn't trigger), and
//! - a burst in the high band (> 500 Hz): the pluck transient. Its short reference window
//!   catches re-plucks after mutes shorter than one period of a low note. It's only a
//!   candidate until the low band confirms a rise (muting a string also makes a click).
//!
//! The note ends when its short-term RMS falls a set amount below its peak, or when the
//! next onset arrives.

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
}

impl Default for NoteDetectorConfig {
    fn default() -> Self {
        Self { gate_db: -45.0, onset_rise_db: 5.0, min_gap_ms: 45.0, release_db: 24.0, end_offset_ms: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteEvent {
    /// `sample` counts from the first sample ever passed to the detector.
    On { sample: u64, level_db: f32 },
    Off { sample: u64, peak_db: f32 },
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

pub struct NoteDetector {
    cfg: NoteDetectorConfig,
    hpf: Biquad,
    hf: [Biquad; 2],
    hop: usize,
    hop_buf: Vec<f32>,
    hf_buf: Vec<f32>,
    hf_hist: VecDeque<f32>,
    hf_hist_len: usize,
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
    energy_hist: VecDeque<f32>,
    rms_hops: usize,
    active: Option<Active>,
    last_onset: Option<u64>,
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
            hf: [Biquad::highpass(sr, 500.0, 0.54), Biquad::highpass(sr, 500.0, 1.31)],
            hop,
            hop_buf: Vec::with_capacity(hop),
            hf_buf: Vec::with_capacity(hop),
            hf_hist: VecDeque::new(),
            hf_hist_len: hops(15.0),
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
            energy_hist: VecDeque::new(),
            rms_hops: hops(40.0),
            active: None,
            last_onset: None,
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
                self.process_hop(out);
                self.next_sample += self.hop as u64;
                self.hop_buf.clear();
                self.hf_buf.clear();
            }
        }
    }

    fn emit_onset(&mut self, sample: u64, level: f32, out: &mut Vec<NoteEvent>) {
        if let Some(a) = self.active.take() {
            out.push(NoteEvent::Off { sample, peak_db: db(a.peak) });
        }
        out.push(NoteEvent::On { sample, level_db: db(level) });
        self.active = Some(Active { peak: level, peak_rms: 0.0 });
        self.last_onset = Some(sample);
    }

    fn process_hop(&mut self, out: &mut Vec<NoteEvent>) {
        let hop_start = self.next_sample;
        let hop_peak = self.hop_buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let energy: f32 = self.hop_buf.iter().map(|v| v * v).sum();
        self.env = hop_peak.max(self.env * self.release);

        // Peaks of the recent past (long enough to span a full period of the lowest note)
        // vs. the peak of the last couple of hops.
        let n = self.peak_hist.len();
        let reference = self.peak_hist.iter().take(n.saturating_sub(self.hist_skip)).fold(0.0f32, |m, &v| m.max(v));
        let current = self.peak_hist.back().copied().unwrap_or(0.0).max(hop_peak);
        let gate = from_db(self.cfg.gate_db);
        let min_gap = (self.cfg.min_gap_ms / 1000.0 * self.sr) as u64;

        let hf_peak = self.hf_buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let n = self.hf_hist.len();
        let hf_ref = self.hf_hist.iter().take(n.saturating_sub(self.hf_skip)).fold(0.0f32, |m, &v| m.max(v));
        let hf_onset = hf_peak > gate * 0.25 && hf_peak > hf_ref * from_db(self.cfg.onset_rise_db + 6.0) && n >= self.hf_hist_len;
        let lf_onset = current > reference * from_db(self.cfg.onset_rise_db);
        self.hf_hist.push_back(hf_peak);
        if self.hf_hist.len() > self.hf_hist_len {
            self.hf_hist.pop_front();
        }

        let rise = from_db(self.cfg.onset_rise_db);
        let floor = self.peak_hist.iter().rev().take(self.floor_hops).fold(0.0f32, |m, &v| m.max(v));
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
        if current > gate && lf_onset && gap_ok {
            let threshold = (hop_peak * 0.3).max(gate * 0.5);
            let idx = self.hop_buf.iter().position(|v| v.abs() >= threshold).unwrap_or(0);
            self.candidate = None;
            self.emit_onset(hop_start + idx as u64, hop_peak, out);
        } else if hf_onset && gap_ok && self.candidate.is_none() {
            let idx = self.hf_buf.iter().position(|v| v.abs() >= hf_peak * 0.3).unwrap_or(0);
            self.candidate = Some(Candidate { sample: hop_start + idx as u64, floor, hops_left: self.confirm_hops });
        }

        self.peak_hist.push_back(hop_peak);
        if self.peak_hist.len() > self.hist_len {
            self.peak_hist.pop_front();
        }
        self.energy_hist.push_back(energy);
        if self.energy_hist.len() > self.rms_hops {
            self.energy_hist.pop_front();
        }
        let rms = (self.energy_hist.iter().sum::<f32>() / (self.energy_hist.len() * self.hop) as f32).sqrt();

        if let Some(a) = &mut self.active {
            a.peak = a.peak.max(hop_peak);
            a.peak_rms = a.peak_rms.max(rms);
            let settled = self.last_onset.is_some_and(|t| hop_start > t + (self.rms_hops * self.hop) as u64);
            if settled && (rms < a.peak_rms * from_db(-self.cfg.release_db) || self.env < gate * 0.5) {
                // With a release threshold this far down, the RMS window is almost entirely
                // past the end of the note when it triggers; place the end at its start.
                let end = (hop_start + self.hop as u64) as i64 - (self.rms_hops * self.hop) as i64 + (self.cfg.end_offset_ms / 1000.0 * self.sr) as i64;
                let onset = self.last_onset.unwrap_or(0) as i64 + 1;
                let sample = end.max(onset) as u64;
                out.push(NoteEvent::Off { sample, peak_db: db(a.peak) });
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
                let damp = if t > dur { (-(t - dur) / 0.006).exp() } else { 1.0 };
                let tone: f32 = (1..6).map(|h| (TAU * freq * h as f32 * t).sin() / h as f32).sum();
                // Finger / pick transient: a short broadband click.
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let click = ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * (-t / 0.0015).exp() * 0.3;
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
        let notes = [(0.25, 0.4, 41.2, 0.5), (0.75, 0.2, 55.0, 0.3), (1.0, 0.45, 61.7, 0.6), (1.5, 0.3, 30.87, 0.4)];
        let ev = detect(sr, &render(sr, 2.5, &notes));
        let ons: Vec<f32> = ev.iter().filter_map(|e| if let NoteEvent::On { sample, .. } = e { Some(*sample as f32 / sr) } else { None }).collect();
        let offs: Vec<f32> = ev.iter().filter_map(|e| if let NoteEvent::Off { sample, .. } = e { Some(*sample as f32 / sr) } else { None }).collect();
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
        assert_eq!(ev.iter().filter(|e| matches!(e, NoteEvent::On { .. })).count(), 1, "{ev:?}");
        // Legato: second note starts while the first still rings.
        let mut sig = render(sr, 1.5, &[(0.1, 1.2, 41.2, 0.3)]);
        let second = render(sr, 1.5, &[(0.6, 0.6, 55.0, 0.6)]);
        sig.iter_mut().zip(&second).for_each(|(a, b)| *a += b);
        let ev = detect(sr, &sig);
        let ons: Vec<_> = ev.iter().filter(|e| matches!(e, NoteEvent::On { .. })).collect();
        assert_eq!(ons.len(), 2, "{ev:?}");
    }

    #[test]
    fn repeated_note_after_short_mute() {
        // Same pitch and level, the previous note muted only ~20 ms before the next one.
        let sr = 48_000.0;
        let notes = [(0.25, 0.23, 41.2, 0.5), (0.5, 0.23, 41.2, 0.5), (0.75, 0.23, 41.2, 0.5)];
        let ev = detect(sr, &render(sr, 1.5, &notes));
        let ons: Vec<f32> = ev.iter().filter_map(|e| if let NoteEvent::On { sample, .. } = e { Some(*sample as f32 / sr) } else { None }).collect();
        assert_eq!(ons.len(), 3, "{ev:?}");
        for (on, n) in ons.iter().zip(&notes) {
            assert!((on - n.0).abs() < 0.003, "onset {on} vs {}", n.0);
        }
    }

    #[test]
    fn end_offset_shifts_note_ends() {
        let sr = 48_000.0;
        let sig = render(sr, 1.0, &[(0.2, 0.3, 55.0, 0.5)]);
        let end = |offset_ms: f32| {
            let mut d = NoteDetector::new(sr, NoteDetectorConfig { end_offset_ms: offset_ms, ..Default::default() });
            let mut ev = Vec::new();
            d.process(&sig, &mut ev);
            ev.iter().find_map(|e| if let NoteEvent::Off { sample, .. } = e { Some(*sample as f32 / sr) } else { None }).unwrap()
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
