//! Mono audio held in memory: [`SampleWriter`] appends to a capped buffer, [`SampleBuffer`]
//! is the finished, immutable audio with min/max peaks for drawing, and [`SampleVoice`]
//! plays part of it back.
//!
//! Sample indices are absolute: they keep counting from the first sample ever written,
//! so they stay valid after a capped writer has dropped older audio.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::{SoundEvent, Voice};

/// Samples summarised by one min/max peak. Blocks hold a whole number of these.
const PEAK_BIN: usize = 256;

/// Finished audio. Cheap to read from any thread.
#[derive(Debug)]
pub struct SampleBuffer {
    sample_rate: f32,
    block_len: usize,
    first: u64,
    end: u64,
    blocks: Vec<Vec<f32>>,
    peaks: Vec<(f32, f32)>,
}

impl SampleBuffer {
    pub fn from_samples(samples: &[f32], sample_rate: f32) -> Self {
        let mut w = SampleWriter::new(sample_rate, usize::MAX);
        w.push(samples);
        w.finish()
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Index of the oldest sample held.
    pub fn first(&self) -> u64 {
        self.first
    }

    /// Index just past the newest sample.
    pub fn end(&self) -> u64 {
        self.end
    }

    pub fn is_empty(&self) -> bool {
        self.first == self.end
    }

    /// The sample at `index`, or silence outside the buffer.
    #[inline]
    pub fn get(&self, index: u64) -> f32 {
        if index < self.first || index >= self.end {
            return 0.0;
        }
        let i = (index - self.first) as usize;
        self.blocks[i / self.block_len][i % self.block_len]
    }

    /// Copies samples `from..to`, clamped to what's held.
    pub fn copy_range(&self, from: u64, to: u64) -> Vec<f32> {
        (from.max(self.first)..to.min(self.end))
            .map(|i| self.get(i))
            .collect()
    }

    /// Lowest and highest sample in `from..to`, or `None` outside the buffer. Long ranges
    /// are read from the peaks, so they may reach slightly past the range.
    pub fn peak_range(&self, from: u64, to: u64) -> Option<(f32, f32)> {
        let (from, to) = (from.max(self.first), to.min(self.end));
        if from >= to {
            return None;
        }
        let fold = |(lo, hi): (f32, f32), (a, b): (f32, f32)| (lo.min(a), hi.max(b));
        let start = (f32::INFINITY, f32::NEG_INFINITY);
        if to - from < 2 * PEAK_BIN as u64 {
            return Some(
                (from..to)
                    .map(|i| self.get(i))
                    .map(|s| (s, s))
                    .fold(start, fold),
            );
        }
        let first_bin = (from - self.first) as usize / PEAK_BIN;
        let last_bin = (to - 1 - self.first) as usize / PEAK_BIN;
        Some(
            self.peaks[first_bin..=last_bin]
                .iter()
                .copied()
                .fold(start, fold),
        )
    }
}

/// Appends audio in fixed blocks, dropping the oldest past `max_bytes`. Memory grows with
/// what's written; nothing is allocated up front.
pub struct SampleWriter {
    sample_rate: f32,
    block_len: usize,
    max_blocks: usize,
    first: u64,
    total: u64,
    blocks: VecDeque<Vec<f32>>,
    peaks: VecDeque<(f32, f32)>,
    bin: Option<(f32, f32)>,
    bin_len: usize,
}

impl SampleWriter {
    pub fn new(sample_rate: f32, max_bytes: usize) -> Self {
        // About a second per block.
        let block_len = (sample_rate.max(1.0) as usize).div_ceil(PEAK_BIN).max(1) * PEAK_BIN;
        let block_bytes = block_len * size_of::<f32>();
        Self {
            sample_rate,
            block_len,
            max_blocks: (max_bytes / block_bytes).max(2),
            first: 0,
            total: 0,
            blocks: VecDeque::new(),
            peaks: VecDeque::new(),
            bin: None,
            bin_len: 0,
        }
    }

    /// Index the next pushed sample gets.
    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn push(&mut self, mut samples: &[f32]) {
        while !samples.is_empty() {
            if self.blocks.back().is_none_or(|b| b.len() == self.block_len) {
                self.add_block();
            }
            let Some(block) = self.blocks.back_mut() else {
                return;
            };
            let (part, rest) = samples.split_at((self.block_len - block.len()).min(samples.len()));
            block.extend_from_slice(part);
            samples = rest;
            self.total += part.len() as u64;
            for &s in part {
                let (lo, hi) = self.bin.unwrap_or((s, s));
                self.bin = Some((lo.min(s), hi.max(s)));
                self.bin_len += 1;
                if self.bin_len == PEAK_BIN {
                    self.flush_bin();
                }
            }
        }
    }

    /// Appends `frames` of silence.
    pub fn push_silence(&mut self, frames: usize) {
        const ZEROS: [f32; 1024] = [0.0; 1024];
        let mut left = frames;
        while left > 0 {
            let n = left.min(ZEROS.len());
            self.push(&ZEROS[..n]);
            left -= n;
        }
    }

    fn add_block(&mut self) {
        if self.blocks.len() == self.max_blocks {
            self.blocks.pop_front();
            self.peaks.drain(..self.block_len / PEAK_BIN);
            self.first += self.block_len as u64;
        }
        self.blocks.push_back(Vec::with_capacity(self.block_len));
    }

    fn flush_bin(&mut self) {
        if let Some(bin) = self.bin.take() {
            self.peaks.push_back(bin);
        }
        self.bin_len = 0;
    }

    pub fn finish(mut self) -> SampleBuffer {
        self.flush_bin();
        SampleBuffer {
            sample_rate: self.sample_rate,
            block_len: self.block_len,
            first: self.first,
            end: self.total,
            blocks: self.blocks.into(),
            peaks: self.peaks.into(),
        }
    }
}

/// Plays samples `start..end` of a buffer once per trigger, resampled to the output rate.
/// The event's gain applies; its kind is ignored.
pub struct SampleVoice {
    audio: Arc<SampleBuffer>,
    start: f64,
    end: f64,
    step: f64,
    pos: Option<f64>,
    gain: f32,
}

impl SampleVoice {
    pub fn new(audio: Arc<SampleBuffer>, start: u64, end: u64, output_rate: f32) -> Self {
        let step = audio.sample_rate() as f64 / output_rate.max(1.0) as f64;
        Self {
            audio,
            start: start as f64,
            end: end as f64,
            step,
            pos: None,
            gain: 1.0,
        }
    }
}

impl Voice for SampleVoice {
    fn trigger(&mut self, ev: &SoundEvent) {
        self.pos = Some(self.start);
        self.gain = ev.gain;
    }

    fn render(&mut self, out: &mut [f32]) {
        let Some(mut pos) = self.pos else { return };
        for o in out {
            if pos >= self.end {
                self.pos = None;
                return;
            }
            let i = pos as u64;
            let (a, b) = (self.audio.get(i), self.audio.get(i + 1));
            *o += (a + (b - a) * (pos - i as f64) as f32) * self.gain;
            pos += self.step;
        }
        self.pos = Some(pos);
    }

    fn silence(&mut self) {
        self.pos = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SoundKind;
    use bg_core::plan::ClickKind;

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| i as f32).collect()
    }

    fn trigger(v: &mut SampleVoice) {
        v.trigger(&SoundEvent {
            kind: SoundKind::Click(ClickKind::Beat),
            gain: 1.0,
        });
    }

    #[test]
    fn reads_across_blocks() {
        let b = SampleBuffer::from_samples(&ramp(3000), 1000.0);
        assert_eq!((b.first(), b.end()), (0, 3000));
        assert_eq!(b.get(2500), 2500.0);
        assert_eq!(b.get(3000), 0.0);
        assert_eq!(b.copy_range(1022, 1026), [1022.0, 1023.0, 1024.0, 1025.0]);
    }

    #[test]
    fn oldest_audio_is_dropped_past_the_cap() {
        // 1024-sample blocks of 4 bytes; a cap of 3 blocks.
        let mut w = SampleWriter::new(1000.0, 3 * 1024 * 4);
        w.push(&ramp(5000));
        let b = w.finish();
        assert_eq!(b.end(), 5000);
        assert_eq!(b.first(), 2048);
        assert_eq!(b.get(2047), 0.0);
        assert_eq!(b.get(2048), 2048.0);
        assert_eq!(b.get(4999), 4999.0);
        // Peaks stay aligned after dropping.
        assert_eq!(b.peak_range(2048, 3072), Some((2048.0, 3071.0)));
    }

    #[test]
    fn peaks_short_and_long_ranges() {
        let mut s = vec![0.0; 2048];
        s[1000] = 0.5;
        s[1001] = -0.75;
        let b = SampleBuffer::from_samples(&s, 1000.0);
        assert_eq!(b.peak_range(1000, 1002), Some((-0.75, 0.5)));
        assert_eq!(b.peak_range(0, 2048), Some((-0.75, 0.5)));
        assert_eq!(b.peak_range(0, 700), Some((0.0, 0.0)));
        assert_eq!(b.peak_range(3000, 4000), None);
    }

    #[test]
    fn partial_last_bin_has_a_peak() {
        let b = SampleBuffer::from_samples(&ramp(1000), 1000.0);
        assert_eq!(b.peak_range(0, 1000), Some((0.0, 999.0)));
    }

    #[test]
    fn silence_counts_as_samples() {
        let mut w = SampleWriter::new(1000.0, usize::MAX);
        w.push(&[1.0]);
        w.push_silence(3000);
        w.push(&[2.0]);
        assert_eq!(w.total(), 3002);
        let b = w.finish();
        assert_eq!((b.get(0), b.get(1500), b.get(3001)), (1.0, 0.0, 2.0));
    }

    #[test]
    fn voice_plays_its_range_once() {
        let audio = Arc::new(SampleBuffer::from_samples(&ramp(100), 1000.0));
        let mut v = SampleVoice::new(audio, 10, 13, 1000.0);
        let mut out = [0.0; 5];
        v.render(&mut out);
        assert_eq!(out, [0.0; 5]); // Not triggered yet.
        trigger(&mut v);
        v.render(&mut out);
        assert_eq!(out, [10.0, 11.0, 12.0, 0.0, 0.0]);
        let mut again = [0.0; 2];
        v.render(&mut again);
        assert_eq!(again, [0.0; 2]);
    }

    #[test]
    fn voice_resamples_to_the_output_rate() {
        let audio = Arc::new(SampleBuffer::from_samples(&ramp(100), 1000.0));
        let mut v = SampleVoice::new(audio.clone(), 0, 100, 2000.0);
        trigger(&mut v);
        let mut out = [0.0; 4];
        v.render(&mut out);
        assert_eq!(out, [0.0, 0.5, 1.0, 1.5]);
        let mut v = SampleVoice::new(audio, 0, 100, 500.0);
        trigger(&mut v);
        out = [0.0; 4];
        v.render(&mut out);
        assert_eq!(out, [0.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn voice_mixes_and_applies_gain() {
        let audio = Arc::new(SampleBuffer::from_samples(&[1.0, 1.0], 1000.0));
        let mut v = SampleVoice::new(audio, 0, 2, 1000.0);
        v.trigger(&SoundEvent {
            kind: SoundKind::Click(ClickKind::Beat),
            gain: 0.5,
        });
        let mut out = [1.0, 1.0];
        v.render(&mut out);
        assert_eq!(out, [1.5, 1.5]);
    }
}
