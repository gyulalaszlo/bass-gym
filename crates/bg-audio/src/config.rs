use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use bg_dsp::NoteDetectorConfig;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// cpal device id (`DeviceId` display form); `None` = system default.
    pub input_device: Option<String>,
    /// 0-based input channel to analyse; `None` = mix all channels.
    pub input_channel: Option<u16>,
    pub output_device: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_frames: Option<u32>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            input_channel: Some(0),
            output_device: None,
            sample_rate: None,
            buffer_frames: Some(256),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisConfig {
    /// Latency correction, in ms: the part of the round trip (output → player → input)
    /// that the host's reported latencies don't account for. Negative when the driver
    /// over-reports its latency.
    pub latency_ms: f32,
    /// Deprecated manual trim (older settings). Read but never written; [`Self::normalize`]
    /// folds it into `latency_ms`.
    #[serde(skip_serializing)]
    pub latency_trim_ms: f32,
    pub detector: NoteDetectorConfig,
    pub tuner_gate_db: f32,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            latency_ms: 0.0,
            latency_trim_ms: 0.0,
            detector: NoteDetectorConfig::default(),
            tuner_gate_db: -60.0,
        }
    }
}

impl AnalysisConfig {
    pub fn total_latency(&self) -> f64 {
        (self.latency_ms + self.latency_trim_ms) as f64 / 1000.0
    }

    /// Folds the deprecated trim into the latency correction.
    pub fn normalize(&mut self) {
        self.latency_ms += self.latency_trim_ms;
        self.latency_trim_ms = 0.0;
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    pub click: String,
    pub groove: String,
    pub instrument: String,
    pub master: f32,
    /// Mix the bass input into the output.
    pub monitor: bool,
    pub monitor_gain: f32,
    /// Low-pass cutoff (Hz) per instrument id; at or above [`LOWPASS_OFF`] = no filter.
    pub instrument_lowpass: BTreeMap<String, f32>,
}

/// Cutoffs at or above this leave the instrument unfiltered.
pub const LOWPASS_OFF: f32 = 16_000.0;

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            click: "synth.click".into(),
            groove: "synth.pluck".into(),
            instrument: "sf2:e-piano-mellow".into(),
            master: 0.8,
            monitor: false,
            monitor_gain: 0.8,
            instrument_lowpass: BTreeMap::new(),
        }
    }
}

impl SoundConfig {
    /// The low-pass cutoff for the current instrument (Hz), if filtered.
    pub fn lowpass(&self) -> Option<f32> {
        let hz = self.lowpass_setting(&self.instrument);
        (hz < LOWPASS_OFF).then_some(hz)
    }

    /// The stored cutoff for an instrument, or its default: the bright FM e-piano is
    /// softened, sampled instruments are left alone.
    pub fn lowpass_setting(&self, instrument: &str) -> f32 {
        self.instrument_lowpass.get(instrument).copied().unwrap_or(
            if instrument == "synth.epiano" {
                2_500.0
            } else {
                LOWPASS_OFF
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_folds_trim_into_correction() {
        let mut c: AnalysisConfig =
            serde_json::from_str(r#"{"latency_ms": 0.8, "latency_trim_ms": -8.0}"#).unwrap();
        let total = c.total_latency();
        c.normalize();
        assert!((c.latency_ms - -7.2).abs() < 1e-4);
        assert_eq!(c.latency_trim_ms, 0.0);
        assert!((c.total_latency() - total).abs() < 1e-9);
        assert!(!serde_json::to_string(&c).unwrap().contains("trim"));
    }
}
