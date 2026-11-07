//! Keeping hold of a session's recording: the size limit preference, and what an
//! exercise needs to fetch the recording once its session has ended.

use std::sync::Arc;

use bg_audio::{AudioEngine, Recording, SessionId};
use serde::{Deserialize, Serialize};

/// Limits of the recording buffer, in MB.
pub const MAX_MB_RANGE: std::ops::RangeInclusive<u32> = 16..=4096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordingPrefs {
    /// Most audio kept per session, in MB. Past this the oldest audio is overwritten.
    pub max_mb: u32,
}

impl Default for RecordingPrefs {
    fn default() -> Self {
        Self { max_mb: 512 }
    }
}

impl RecordingPrefs {
    pub const ID: &'static str = "recording";

    pub fn max_bytes(&self) -> usize {
        self.max_mb as usize * 1024 * 1024
    }

    /// Minutes of audio that fit, at `sample_rate`.
    pub fn minutes(&self, sample_rate: u32) -> f32 {
        self.max_bytes() as f32 / (sample_rate.max(1) as f32 * 4.0 * 60.0)
    }
}

/// The recording of an exercise's latest session.
#[derive(Default)]
pub struct SessionCapture {
    pending: Option<SessionId>,
    recording: Option<Arc<Recording>>,
}

impl SessionCapture {
    /// A new session started; the previous recording is released.
    pub fn begin(&mut self, id: SessionId) {
        self.pending = Some(id);
        self.recording = None;
    }

    /// Picks up the recording once the session has ended. Call on every frame.
    pub fn poll(&mut self, engine: &AudioEngine) {
        if let Some(id) = self.pending
            && let Some(r) = engine.take_recording(id)
        {
            self.pending = None;
            self.recording = Some(r);
        }
    }

    pub fn recording(&self) -> Option<&Arc<Recording>> {
        self.recording.as_ref()
    }

    /// Takes a recording made some other way (a debugging session).
    pub fn set(&mut self, recording: Arc<Recording>) {
        self.pending = None;
        self.recording = Some(recording);
    }

    /// Releases the recording, freeing its memory.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cap_holds_over_40_minutes_at_48k() {
        let prefs = RecordingPrefs::default();
        assert_eq!(prefs.max_bytes(), 512 * 1024 * 1024);
        assert!(prefs.minutes(48_000) > 40.0);
    }
}
