//! The transport clock shared between the output callback, the worker and the UI.
//!
//! Transport time is `output frame index / sample rate`, where frame `n` is the one that
//! reaches the DAC at the "playback" instant reported by the host. Input capture
//! instants are mapped onto the same timeline, so played notes and scheduled sounds are
//! directly comparable.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ClockSnapshot {
    /// Index of the first frame of the latest output callback.
    pub frames: u64,
    pub sample_rate: u32,
    /// Host playback instant of that frame (host stream clock), ns.
    pub playback_ns: u64,
    /// Host callback instant (host stream clock), ns.
    pub callback_ns: u64,
    /// `Instant` of the callback, ns since the clock's epoch.
    pub local_ns: u64,
}

#[derive(Default)]
struct Inner {
    seq: AtomicU32,
    frames: AtomicU64,
    sample_rate: AtomicU64,
    playback_ns: AtomicU64,
    callback_ns: AtomicU64,
    local_ns: AtomicU64,
    running: AtomicBool,
}

#[derive(Clone)]
pub struct Clock {
    inner: Arc<Inner>,
    epoch: Instant,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            epoch: Instant::now(),
        }
    }
}

impl Clock {
    pub(crate) fn local_ns(&self) -> u64 {
        self.epoch.elapsed().as_nanos() as u64
    }

    /// Single writer: the output callback.
    pub(crate) fn publish(&self, s: ClockSnapshot) {
        let i = &self.inner;
        let seq = i.seq.load(Ordering::Relaxed);
        i.seq.store(seq.wrapping_add(1), Ordering::Release);
        std::sync::atomic::fence(Ordering::Release);
        i.frames.store(s.frames, Ordering::Relaxed);
        i.sample_rate.store(s.sample_rate as u64, Ordering::Relaxed);
        i.playback_ns.store(s.playback_ns, Ordering::Relaxed);
        i.callback_ns.store(s.callback_ns, Ordering::Relaxed);
        i.local_ns.store(s.local_ns, Ordering::Relaxed);
        i.seq.store(seq.wrapping_add(2), Ordering::Release);
        i.running.store(true, Ordering::Release);
    }

    pub(crate) fn set_stopped(&self) {
        self.inner.running.store(false, Ordering::Release);
    }

    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::Acquire)
    }

    pub(crate) fn snapshot(&self) -> Option<ClockSnapshot> {
        let i = &self.inner;
        if !self.is_running() {
            return None;
        }
        loop {
            let s1 = i.seq.load(Ordering::Acquire);
            if s1 % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let snap = ClockSnapshot {
                frames: i.frames.load(Ordering::Relaxed),
                sample_rate: i.sample_rate.load(Ordering::Relaxed) as u32,
                playback_ns: i.playback_ns.load(Ordering::Relaxed),
                callback_ns: i.callback_ns.load(Ordering::Relaxed),
                local_ns: i.local_ns.load(Ordering::Relaxed),
            };
            std::sync::atomic::fence(Ordering::Acquire);
            if i.seq.load(Ordering::Relaxed) == s1 {
                return Some(snap);
            }
        }
    }

    /// Transport time currently being heard (estimated from the last callback and the
    /// host-reported output latency). Returns 0 when output isn't running.
    pub fn now(&self) -> f64 {
        let Some(s) = self.snapshot() else { return 0.0 };
        let sr = s.sample_rate.max(1) as f64;
        let since_callback = self.local_ns().saturating_sub(s.local_ns) as f64 * 1e-9;
        let out_latency = s.playback_ns.saturating_sub(s.callback_ns) as f64 * 1e-9;
        s.frames as f64 / sr + since_callback - out_latency
    }

    /// Maps a host-clock capture instant (ns) to transport time.
    pub(crate) fn map_host_ns(&self, capture_ns: u64) -> Option<f64> {
        let s = self.snapshot()?;
        if capture_ns == 0 || s.playback_ns == 0 {
            return None;
        }
        let sr = s.sample_rate.max(1) as f64;
        Some(s.frames as f64 / sr + (capture_ns as f64 - s.playback_ns as f64) * 1e-9)
    }
}
