//! The recording of a practice session: the raw bass input, a map from its samples to
//! transport time, and the sounds that were scheduled while it ran.

use std::sync::Arc;

use bg_sound::{SampleBuffer, SampleWriter, SoundEvent, SoundKind};

use crate::session::Slot;

/// How often to store a time anchor, in seconds.
const ANCHOR_INTERVAL: f64 = 0.25;
/// A mapping this far from the running anchor gets a new anchor at once (offset snaps,
/// latency changes, dropped input).
const ANCHOR_TOLERANCE: f64 = 0.001;

/// Identifies a recorded session, to fetch its recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionId(pub(crate) u64);

/// A sound scheduled during a session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoggedSound {
    pub at: f64,
    pub slot: Slot,
    pub ev: SoundEvent,
    /// Reached the output queue; false when output was missing or the queue was full.
    pub delivered: bool,
}

/// Input audio lost to a stalled worker, replaced by silence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gap {
    pub time: f64,
    pub seconds: f64,
}

/// Transport time of a sample. Later samples count on from it until the next anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Anchor {
    sample: u64,
    time: f64,
}

/// A finished recording. Times are transport times with latency compensated, the same
/// as note times, so markers line up with the audio.
#[derive(Debug)]
pub struct Recording {
    pub audio: Arc<SampleBuffer>,
    pub gaps: Vec<Gap>,
    pub sounds: Vec<LoggedSound>,
    /// Transport time the session ended.
    pub stop_time: f64,
    /// Latency subtracted from input times when the session ended, in seconds.
    pub latency: f64,
    anchors: Vec<Anchor>,
}

impl Recording {
    /// Whether any input was captured (not the case without an input device).
    pub fn has_audio(&self) -> bool {
        !self.anchors.is_empty() && !self.audio.is_empty()
    }

    /// Transport time of sample `sample`.
    pub fn time_of(&self, sample: u64) -> f64 {
        let sr = self.audio.sample_rate().max(1.0) as f64;
        let i = self.anchors.partition_point(|a| a.sample <= sample);
        match self.anchors.get(i.saturating_sub(1)) {
            Some(a) => a.time + (sample as f64 - a.sample as f64) / sr,
            None => sample as f64 / sr,
        }
    }

    /// The sample at transport time `time`, which may lie outside the audio held.
    pub fn sample_at(&self, time: f64) -> u64 {
        let sr = self.audio.sample_rate().max(1.0) as f64;
        let i = self.anchors.partition_point(|a| a.time <= time);
        let s = match self.anchors.get(i.saturating_sub(1)) {
            Some(a) => a.sample as f64 + (time - a.time) * sr,
            None => time * sr,
        };
        s.round().max(0.0) as u64
    }

    /// Transport time of the oldest audio held. Later than the session start once the
    /// buffer has overwritten earlier audio.
    pub fn first_time(&self) -> f64 {
        self.time_of(self.audio.first())
    }

    pub fn end_time(&self) -> f64 {
        self.time_of(self.audio.end())
    }
}

/// Collects a recording while a session runs.
pub struct Recorder {
    max_bytes: usize,
    writer: Option<SampleWriter>,
    sample_rate: f32,
    anchors: Vec<Anchor>,
    gaps: Vec<Gap>,
    sounds: Vec<LoggedSound>,
}

impl Recorder {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            writer: None,
            sample_rate: 0.0,
            anchors: Vec::new(),
            gaps: Vec::new(),
            sounds: Vec::new(),
        }
    }

    /// Appends input. `time` is the transport time of `samples[0]`, if known yet, and
    /// `dropped` the frames lost just before it, kept as silence.
    pub fn feed(&mut self, samples: &[f32], sample_rate: f32, dropped: u32, time: Option<f64>) {
        let max_bytes = self.max_bytes;
        let writer = self
            .writer
            .get_or_insert_with(|| SampleWriter::new(sample_rate, max_bytes));
        self.sample_rate = sample_rate;
        let sr = sample_rate.max(1.0) as f64;
        if dropped > 0 {
            let seconds = dropped as f64 / sr;
            if let Some(t) = time {
                self.gaps.push(Gap {
                    time: t - seconds,
                    seconds,
                });
            }
            writer.push_silence(dropped as usize);
        }
        let sample = writer.total();
        if let Some(time) = time {
            let due = self.anchors.last().is_none_or(|a| {
                let elapsed = (sample - a.sample) as f64 / sr;
                elapsed >= ANCHOR_INTERVAL || (a.time + elapsed - time).abs() > ANCHOR_TOLERANCE
            });
            if due {
                self.anchors.push(Anchor { sample, time });
            }
        }
        writer.push(samples);
    }

    /// Where a session's scheduled sounds are logged.
    pub fn sounds_mut(&mut self) -> &mut Vec<LoggedSound> {
        &mut self.sounds
    }

    /// Ends the recording at transport time `stop_time`: sounds scheduled beyond it never
    /// played, and notes sounding at that moment were cut off.
    pub fn finish(mut self, stop_time: f64, latency: f64) -> Recording {
        self.sounds.retain(|s| s.at <= stop_time);
        for s in &mut self.sounds {
            if let SoundKind::Note { duration, .. } = &mut s.ev.kind {
                *duration = duration.min((stop_time - s.at) as f32);
            }
        }
        let audio = match self.writer {
            Some(w) => w.finish(),
            None => SampleBuffer::from_samples(&[], self.sample_rate),
        };
        Recording {
            audio: Arc::new(audio),
            gaps: self.gaps,
            sounds: self.sounds,
            stop_time,
            latency,
            anchors: self.anchors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_core::plan::ClickKind;

    const SR: f32 = 1000.0;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    fn note(at: f64, duration: f32) -> LoggedSound {
        LoggedSound {
            at,
            slot: Slot::Instrument,
            ev: SoundEvent {
                kind: SoundKind::Note {
                    midi: 60.0,
                    velocity: 1.0,
                    duration,
                },
                gain: 1.0,
            },
            delivered: true,
        }
    }

    #[test]
    fn maps_samples_to_time_and_back() {
        let mut r = Recorder::new(usize::MAX);
        r.feed(&[0.0; 100], SR, 0, Some(10.0));
        r.feed(&[0.0; 100], SR, 0, Some(10.1));
        let rec = r.finish(20.0, 0.0);
        assert!(rec.has_audio());
        close(rec.time_of(150), 10.15);
        assert_eq!(rec.sample_at(10.15), 150);
        close(rec.first_time(), 10.0);
        close(rec.end_time(), 10.2);
    }

    #[test]
    fn audio_before_the_time_is_known_counts_back_from_the_first_anchor() {
        let mut r = Recorder::new(usize::MAX);
        r.feed(&[0.0; 100], SR, 0, None);
        r.feed(&[0.0; 100], SR, 0, Some(5.0));
        let rec = r.finish(9.0, 0.0);
        close(rec.time_of(0), 4.9);
    }

    #[test]
    fn a_time_snap_starts_a_new_anchor_at_once() {
        let mut r = Recorder::new(usize::MAX);
        r.feed(&[0.0; 100], SR, 0, Some(10.0));
        r.feed(&[0.0; 100], SR, 0, Some(10.5)); // The offset jumped by 0.4 s.
        let rec = r.finish(20.0, 0.0);
        close(rec.time_of(99), 10.099);
        close(rec.time_of(100), 10.5);
        assert_eq!(rec.sample_at(10.05), 50);
        assert_eq!(rec.sample_at(10.55), 150);
    }

    #[test]
    fn small_drift_does_not_add_anchors_until_the_interval() {
        let mut r = Recorder::new(usize::MAX);
        r.feed(&[0.0; 100], SR, 0, Some(10.0));
        r.feed(&[0.0; 100], SR, 0, Some(10.1005));
        r.feed(&[0.0; 100], SR, 0, Some(10.2));
        assert_eq!(r.anchors.len(), 1);
        r.feed(&[0.0; 100], SR, 0, Some(10.3));
        assert_eq!(r.anchors.len(), 2);
    }

    #[test]
    fn dropped_input_becomes_silence_and_a_gap() {
        let mut r = Recorder::new(usize::MAX);
        r.feed(&[1.0; 100], SR, 0, Some(10.0));
        r.feed(&[1.0; 100], SR, 50, Some(10.15));
        let rec = r.finish(20.0, 0.0);
        assert_eq!(rec.audio.end(), 250);
        assert_eq!(rec.audio.get(120), 0.0);
        assert_eq!(rec.audio.get(150), 1.0);
        close(rec.time_of(150), 10.15);
        assert_eq!(rec.gaps.len(), 1);
        close(rec.gaps[0].time, 10.1);
        close(rec.gaps[0].seconds, 0.05);
    }

    #[test]
    fn overwritten_audio_moves_the_first_time() {
        // 1024-sample blocks, 2 kept.
        let mut r = Recorder::new(2 * 1024 * 4);
        for i in 0..50 {
            r.feed(&[0.5; 100], SR, 0, Some(i as f64 * 0.1));
        }
        let rec = r.finish(10.0, 0.0);
        assert!(rec.audio.first() > 0);
        close(rec.first_time(), rec.audio.first() as f64 / SR as f64);
        close(rec.end_time(), 5.0);
    }

    #[test]
    fn stopping_drops_later_sounds_and_cuts_sounding_notes() {
        let mut r = Recorder::new(usize::MAX);
        r.sounds_mut().extend([
            note(1.0, 0.5),
            note(2.0, 4.0),
            LoggedSound {
                ev: SoundEvent {
                    kind: SoundKind::Click(ClickKind::Beat),
                    gain: 1.0,
                },
                ..note(2.5, 0.0)
            },
            note(5.0, 1.0),
        ]);
        let rec = r.finish(3.0, 0.0);
        assert_eq!(rec.sounds.len(), 3);
        let durations: Vec<f32> = rec
            .sounds
            .iter()
            .filter_map(|s| match s.ev.kind {
                SoundKind::Note { duration, .. } => Some(duration),
                _ => None,
            })
            .collect();
        assert_eq!(durations, [0.5, 1.0]);
    }

    #[test]
    fn without_input_only_the_sounds_remain() {
        let mut r = Recorder::new(usize::MAX);
        r.sounds_mut().push(note(1.0, 0.5));
        let rec = r.finish(3.0, 0.01);
        assert!(!rec.has_audio());
        assert_eq!(rec.sounds.len(), 1);
    }
}
