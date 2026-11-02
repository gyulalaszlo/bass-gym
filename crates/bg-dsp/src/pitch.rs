//! McLeod Pitch Method (NSDF) pitch tracker tuned for bass frequencies.
//!
//! The coarse search runs on a decimated (~12 kHz) signal with a long window so that
//! B0 (30.9 Hz) fits several periods; the result is then refined at the full sample
//! rate around the coarse lag, which gives sub-cent accuracy on steady tones.

use crate::db;
use crate::filter::{Biquad, History};

const WINDOW: usize = 2048;
const HOP: usize = 256;
const MIN_FREQ: f32 = 26.0;
const MAX_FREQ: f32 = 1200.0;
const KEY_MAX_THRESHOLD: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchReading {
    pub freq: f32,
    /// NSDF peak height in 0..=1; how periodic the signal is.
    pub clarity: f32,
    pub rms_db: f32,
}

pub struct PitchTracker {
    sr: f32,
    decim: usize,
    lp: [Biquad; 2],
    phase: usize,
    low: History,
    full: History,
    since_hop: usize,
    /// Minimum RMS for a reading.
    pub gate_db: f32,
    /// Minimum clarity for a reading.
    pub min_clarity: f32,
    scratch: Vec<f32>,
    nsdf: Vec<f32>,
    full_scratch: Vec<f32>,
}

impl PitchTracker {
    pub fn new(sr: f32) -> Self {
        let decim = decimation(sr);
        Self {
            sr,
            decim,
            lp: lowpass(sr, decim),
            phase: 0,
            low: History::new(WINDOW),
            full: History::new(WINDOW * decim),
            since_hop: 0,
            gate_db: -60.0,
            min_clarity: 0.8,
            scratch: vec![0.0; WINDOW],
            nsdf: Vec::with_capacity(WINDOW),
            full_scratch: vec![0.0; WINDOW * decim],
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// Feeds samples. Returns `Some(result)` for every completed analysis hop
    /// (only the latest is returned when several complete); the inner `Option` is `None`
    /// when no stable pitch was found (silence / noise).
    pub fn push(&mut self, samples: &[f32]) -> Option<Option<PitchReading>> {
        let mut result = None;
        for &x in samples {
            self.full.push(x);
            let y = self.lp[0].process(x);
            let y = self.lp[1].process(y);
            self.phase += 1;
            if self.phase == self.decim {
                self.phase = 0;
                self.low.push(y);
                self.since_hop += 1;
                if self.since_hop >= HOP {
                    self.since_hop = 0;
                    result = Some(self.analyze());
                }
            }
        }
        result
    }

    fn analyze(&mut self) -> Option<PitchReading> {
        self.low.copy_latest(&mut self.scratch);
        self.full.copy_latest(&mut self.full_scratch);
        estimate(
            &self.scratch,
            &self.full_scratch,
            self.sr,
            self.decim,
            self.gate_db,
            self.min_clarity,
            &mut self.nsdf,
        )
    }
}

fn decimation(sr: f32) -> usize {
    ((sr / 11_025.0).floor() as usize).max(1)
}

fn lowpass(sr: f32, decim: usize) -> [Biquad; 2] {
    let cutoff = (sr / decim as f32 * 0.4).min(MAX_FREQ * 4.0);
    [
        Biquad::lowpass(sr, cutoff, 0.54),
        Biquad::lowpass(sr, cutoff, 1.31),
    ]
}

/// One-shot pitch detection of a whole buffer, e.g. the samples of a single played
/// note. Needs roughly three periods of the note (~75 ms for low E); returns `None`
/// when the buffer is too short or not periodic enough.
pub fn detect(samples: &[f32], sr: f32) -> Option<PitchReading> {
    let decim = decimation(sr);
    let mut lp = lowpass(sr, decim);
    let low: Vec<f32> = samples
        .iter()
        .map(|&x| {
            let y = lp[0].process(x);
            lp[1].process(y)
        })
        .step_by(decim)
        .collect();
    // Skip the filter's settling time.
    let skip = (low.len() / 16).min(64);
    let mut buf = Vec::new();
    estimate(
        &low[skip..],
        &samples[skip * decim..],
        sr,
        decim,
        -70.0,
        0.75,
        &mut buf,
    )
}

/// MPM on a decimated window, refined on the matching full-rate samples.
fn estimate(
    low: &[f32],
    full: &[f32],
    sr: f32,
    decim: usize,
    gate_db: f32,
    min_clarity: f32,
    nsdf_buf: &mut Vec<f32>,
) -> Option<PitchReading> {
    if low.len() < 64 {
        return None;
    }
    let energy: f32 = low.iter().map(|v| v * v).sum();
    let rms_db = db((energy / low.len() as f32).sqrt());
    if rms_db < gate_db {
        return None;
    }
    let low_sr = sr / decim as f32;
    let max_tau = ((low_sr / MIN_FREQ) as usize).min(low.len() / 2);
    let min_tau = ((low_sr / MAX_FREQ) as usize).max(2);
    if max_tau <= min_tau + 2 {
        return None;
    }
    nsdf(low, max_tau + 2, nsdf_buf);
    let (tau, clarity) = pick_peak(nsdf_buf, min_tau, max_tau)?;
    if clarity < min_clarity {
        return None;
    }

    // Refine at full rate around the decimated estimate.
    let est = tau * decim as f32;
    let center = est.round() as usize;
    let span = decim + 1;
    let lo = center.saturating_sub(span).max(1);
    let hi = (center + span).min(full.len() / 2);
    let refined = if hi > lo {
        let vals: Vec<f32> = (lo - 1..=hi + 1).map(|t| nsdf_at(full, t)).collect();
        let mut best = 1;
        for i in 1..vals.len() - 1 {
            if vals[i] > vals[best] {
                best = i;
            }
        }
        if best + 1 < vals.len() {
            (lo - 1 + best) as f32 + parabolic(vals[best - 1], vals[best], vals[best + 1]).0
        } else {
            est
        }
    } else {
        est
    };
    Some(PitchReading {
        freq: sr / refined,
        clarity,
        rms_db,
    })
}

/// Normalized square difference function for lags `0..max_lag`.
fn nsdf(x: &[f32], max_lag: usize, out: &mut Vec<f32>) {
    out.clear();
    let n = x.len();
    let mut m: f32 = 2.0 * x.iter().map(|v| v * v).sum::<f32>();
    for tau in 0..max_lag.min(n) {
        if tau > 0 {
            m -= x[tau - 1] * x[tau - 1] + x[n - tau] * x[n - tau];
        }
        let r: f32 = x[..n - tau].iter().zip(&x[tau..]).map(|(a, b)| a * b).sum();
        out.push(if m > 1e-12 { 2.0 * r / m } else { 0.0 });
    }
}

fn nsdf_at(x: &[f32], tau: usize) -> f32 {
    let n = x.len();
    let (r, m) = x[..n - tau]
        .iter()
        .zip(&x[tau..])
        .fold((0.0f64, 0.0f64), |(r, m), (&a, &b)| {
            (r + (a * b) as f64, m + (a * a + b * b) as f64)
        });
    if m > 1e-12 { (2.0 * r / m) as f32 } else { 0.0 }
}

/// Vertex offset (relative to the middle sample) and height of a parabola through 3 points.
fn parabolic(a: f32, b: f32, c: f32) -> (f32, f32) {
    let d = a - 2.0 * b + c;
    if d.abs() < 1e-12 {
        return (0.0, b);
    }
    let off = 0.5 * (a - c) / d;
    (off, b - 0.25 * (a - c) * off)
}

/// MPM peak picking: key maxima between positive zero crossings; the first one above
/// `KEY_MAX_THRESHOLD * highest` wins.
fn pick_peak(n: &[f32], min_tau: usize, max_tau: usize) -> Option<(f32, f32)> {
    let mut keys: Vec<(usize, f32)> = Vec::new();
    let mut tau = 1;
    // Skip the initial positive lobe around lag 0.
    while tau < n.len() && n[tau] > 0.0 {
        tau += 1;
    }
    let mut cur: Option<(usize, f32)> = None;
    while tau < n.len() - 1 && tau <= max_tau {
        if n[tau] > 0.0 {
            if n[tau] >= n[tau - 1]
                && n[tau] >= n[tau + 1]
                && cur.is_none_or(|c| n[tau] > c.1)
                && tau >= min_tau
            {
                cur = Some((tau, n[tau]));
            }
        } else if let Some(c) = cur.take() {
            keys.push(c);
        }
        tau += 1;
    }
    keys.extend(cur);
    let highest = keys.iter().map(|k| k.1).fold(0.0, f32::max);
    let &(t, _) = keys.iter().find(|k| k.1 >= KEY_MAX_THRESHOLD * highest)?;
    let (off, height) = parabolic(n[t - 1], n[t], n[t + 1]);
    Some((t as f32 + off, height.min(1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn bass_tone(sr: f32, freq: f32, secs: f32) -> Vec<f32> {
        // Harmonic-rich tone with a comparatively weak fundamental, like a real bass.
        let amps = [0.5, 1.0, 0.7, 0.4, 0.3, 0.2, 0.1];
        (0..(sr * secs) as usize)
            .map(|i| {
                let t = i as f32 / sr;
                amps.iter()
                    .enumerate()
                    .map(|(h, a)| a * (TAU * freq * (h + 1) as f32 * t + h as f32).sin())
                    .sum::<f32>()
                    * 0.2
            })
            .collect()
    }

    fn measure(sr: f32, signal: &[f32]) -> Option<PitchReading> {
        let mut t = PitchTracker::new(sr);
        let mut last = None;
        for chunk in signal.chunks(480) {
            if let Some(r) = t.push(chunk) {
                last = r;
            }
        }
        last
    }

    #[test]
    fn accurate_across_bass_range() {
        for sr in [44_100.0, 48_000.0, 96_000.0] {
            for midi in [23, 28, 33, 38, 43, 52, 60, 67] {
                let freq = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
                let r = measure(sr, &bass_tone(sr, freq, 0.6))
                    .unwrap_or_else(|| panic!("no reading for {midi} @ {sr}"));
                let cents = 1200.0 * (r.freq / freq).log2();
                assert!(
                    cents.abs() < 1.0,
                    "midi {midi} @ {sr}: {} Hz vs {freq} ({cents:.2} cents)",
                    r.freq
                );
            }
        }
    }

    #[test]
    fn detuned_tone() {
        let sr = 48_000.0;
        let freq = 41.2034 * 2f32.powf(7.0 / 1200.0);
        let r = measure(sr, &bass_tone(sr, freq, 0.6)).unwrap();
        assert!((1200.0 * (r.freq / 41.2034).log2() - 7.0).abs() < 0.5);
    }

    /// A plucked note: attack transient + decaying harmonics.
    fn pluck(sr: f32, freq: f32, secs: f32) -> Vec<f32> {
        let mut seed = 9u32;
        (0..(sr * secs) as usize)
            .map(|i| {
                let t = i as f32 / sr;
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let click =
                    ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * (-t / 0.0015).exp() * 0.3;
                let tone: f32 = (1..6)
                    .map(|h| (TAU * freq * h as f32 * t).sin() / h as f32)
                    .sum();
                0.4 * ((t / 0.002).min(1.0) * (-t / 1.0).exp() * tone + click)
            })
            .collect()
    }

    #[test]
    fn detects_single_notes_from_onset() {
        let sr = 48_000.0;
        for midi in [23, 28, 33, 40, 45, 52, 60] {
            let freq = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
            for secs in [0.1, 0.4] {
                let r = detect(&pluck(sr, freq, secs), sr)
                    .unwrap_or_else(|| panic!("midi {midi}, {secs}s"));
                let cents = 1200.0 * (r.freq / freq).log2();
                assert!(cents.abs() < 5.0, "midi {midi} {secs}s: {cents} cents");
            }
        }
        // Low E needs ~3 periods; 30 ms isn't enough.
        assert!(detect(&pluck(sr, 41.2, 0.03), sr).is_none());
    }

    #[test]
    fn silence_and_noise_give_no_pitch() {
        let sr = 48_000.0;
        assert!(measure(sr, &vec![0.0; 48_000]).is_none());
        let mut seed = 12345u32;
        let noise: Vec<f32> = (0..48_000)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1 << 24) as f32 - 0.5
            })
            .collect();
        assert!(measure(sr, &noise).is_none());
    }
}
