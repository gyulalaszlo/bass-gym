use serde::{Deserialize, Serialize};

/// Ticks per quarter note.
pub const PPQ: u32 = 960;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeSignature {
    pub num: u32,
    pub den: u32,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self { num: 4, den: 4 }
    }
}

impl TimeSignature {
    pub fn ticks_per_beat(&self) -> u32 {
        PPQ * 4 / self.den.max(1)
    }

    pub fn ticks_per_bar(&self) -> u32 {
        self.ticks_per_beat() * self.num.max(1)
    }

    /// Seconds per tick at `bpm`, where the BPM refers to the signature's beat unit.
    pub fn seconds_per_tick(&self, bpm: f32) -> f64 {
        60.0 / (bpm.max(1.0) as f64) / self.ticks_per_beat() as f64
    }
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Returns the nearest MIDI note and the deviation from it in cents.
pub fn freq_to_midi(freq: f32, a4: f32) -> (i32, f32) {
    let exact = 69.0 + 12.0 * (freq / a4).log2();
    let midi = exact.round();
    (midi as i32, (exact - midi) * 100.0)
}

pub fn midi_to_freq(midi: f32, a4: f32) -> f32 {
    a4 * 2f32.powf((midi - 69.0) / 12.0)
}

pub fn note_name(midi: i32) -> &'static str {
    NOTE_NAMES[midi.rem_euclid(12) as usize]
}

pub fn note_octave(midi: i32) -> i32 {
    midi.div_euclid(12) - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_ticks() {
        let s = TimeSignature { num: 6, den: 8 };
        assert_eq!(s.ticks_per_beat(), PPQ / 2);
        assert_eq!(s.ticks_per_bar(), PPQ * 3);
        let s = TimeSignature::default();
        assert!((s.seconds_per_tick(120.0) * PPQ as f64 - 0.5).abs() < 1e-12);
    }

    #[test]
    fn note_mapping() {
        assert_eq!(freq_to_midi(440.0, 440.0), (69, 0.0));
        let (m, c) = freq_to_midi(41.2034, 440.0);
        assert_eq!(m, 28);
        assert!(c.abs() < 0.01);
        assert_eq!(note_name(28), "E");
        assert_eq!(note_octave(28), 1);
        assert_eq!(note_octave(23), 0); // B0
    }
}
