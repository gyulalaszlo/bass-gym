//! Shared configuration of a tempo + groove practice session. Serialized into the
//! presets of every timing-based exercise type, so every struct is `#[serde(default)]`.

use serde::{Deserialize, Serialize};

use crate::groove::Groove;
use crate::music::TimeSignature;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub tempo: f32,
    pub time_sig: TimeSignature,
    pub groove: Groove,
    pub click: ClickConfig,
    pub groove_click: GrooveClickConfig,
    pub count_in_bars: u32,
    pub ramp: TempoRamp,
    pub gap: ClickGap,
    pub auto_stop: AutoStop,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            tempo: 90.0,
            time_sig: TimeSignature::default(),
            groove: Groove::default(),
            click: ClickConfig::default(),
            groove_click: GrooveClickConfig::default(),
            count_in_bars: 1,
            ramp: TempoRamp::default(),
            gap: ClickGap::default(),
            auto_stop: AutoStop::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickDensity {
    Bar,
    Beat,
    Half,
    Quarter,
}

impl ClickDensity {
    pub const ALL: [ClickDensity; 4] = [Self::Bar, Self::Beat, Self::Half, Self::Quarter];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bar => "Bar",
            Self::Beat => "Beat",
            Self::Half => "1/2 beat",
            Self::Quarter => "1/4 beat",
        }
    }

    /// Clicks per beat (0 = one per bar).
    pub fn per_beat(self) -> u32 {
        match self {
            Self::Bar => 0,
            Self::Beat => 1,
            Self::Half => 2,
            Self::Quarter => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClickConfig {
    pub enabled: bool,
    pub density: ClickDensity,
    pub accent_downbeat: bool,
    pub volume: f32,
}

impl Default for ClickConfig {
    fn default() -> Self {
        Self { enabled: true, density: ClickDensity::Beat, accent_downbeat: true, volume: 0.8 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GrooveClickConfig {
    pub enabled: bool,
    pub volume: f32,
    /// MIDI note played by the groove voice.
    pub pitch: u8,
}

impl Default for GrooveClickConfig {
    fn default() -> Self {
        Self { enabled: false, volume: 0.6, pitch: 40 }
    }
}

/// A length expressed either in bars or in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Span {
    Bars(u32),
    Seconds(f32),
}

impl Default for Span {
    fn default() -> Self {
        Span::Bars(4)
    }
}

/// When something (a tempo step, the end of the session) happens.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Trigger {
    After(Span),
    /// The average hit rate ("good" or "ok" grades) of the last `bars` bars reaches `min_rate`.
    Success { bars: u32, min_rate: f32 },
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger::After(Span::Bars(32))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TempoRamp {
    pub enabled: bool,
    pub target: f32,
    pub step: f32,
    pub every: Trigger,
}

impl Default for TempoRamp {
    fn default() -> Self {
        Self { enabled: false, target: 200.0, step: 5.0, every: Trigger::After(Span::Bars(32)) }
    }
}

/// Play the click(s) for `play`, then silence them for `mute`, and repeat.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClickGap {
    pub enabled: bool,
    pub play: Span,
    pub mute: Span,
}

impl Default for ClickGap {
    fn default() -> Self {
        Self { enabled: false, play: Span::Bars(4), mute: Span::Bars(4) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum StopCondition {
    Trigger(Trigger),
    /// Stop once the tempo ramp reached its target and `bars` more bars were played.
    RampDone { bars: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoStop {
    /// Stop when `when` is met.
    pub enabled: bool,
    pub when: StopCondition,
    /// Independently of `enabled`: stop once nothing was played for `silence_bars` bars.
    pub on_silence: bool,
    pub silence_bars: u32,
}

impl Default for AutoStop {
    fn default() -> Self {
        Self { enabled: false, when: StopCondition::Trigger(Trigger::After(Span::Bars(64))), on_silence: false, silence_bars: 2 }
    }
}
