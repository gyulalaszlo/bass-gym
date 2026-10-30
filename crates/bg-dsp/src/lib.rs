//! Audio analysis: high-accuracy pitch tracking for the tuner and note onset/offset
//! detection for timing evaluation.

pub mod filter;
pub mod notes;
pub mod pitch;

pub use notes::{NoteDetector, NoteDetectorConfig, NoteEvent};
pub use pitch::{PitchReading, PitchTracker, detect};

pub fn db(amplitude: f32) -> f32 {
    20.0 * amplitude.max(1e-9).log10()
}

pub fn from_db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
