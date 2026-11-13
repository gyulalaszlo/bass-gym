//! What Basic timing knows: its preset data, what it read from the outside world this
//! frame, and the state of its latest session.

use bg_core::{ScoringThresholds, SessionConfig};
use bg_ui::recording::SessionCapture;
use bg_ui::view::ViewPrefs;
use bg_ui::views::results::ResultsTab;
use bg_ui::views::review::Review;
use bg_ui::views::tap_tempo;
use serde::{Deserialize, Serialize};

use crate::basic_scales::{CustomScale, ScaleSettings, SharedScale};
use crate::timing_session::SharedTiming;

/// Which exercise this is: Basic timing, or Basic scales (the same, with a pitch for
/// every note: see `basic_scales`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Timing,
    Scales,
}

impl Kind {
    /// The exercise's id. Never change these: presets and settings are stored under them.
    pub fn id(self) -> &'static str {
        match self {
            Kind::Timing => "basic_timing",
            Kind::Scales => "basic_scales",
        }
    }
}

/// The preset data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasicTimingData {
    pub session: SessionConfig,
    pub view: ViewOptions,
    /// What Basic scales plays (none in Basic timing's presets).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<ScaleSettings>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewOptions {
    pub bars_behind: f64,
    pub bars_ahead: f64,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            bars_behind: 1.0,
            bars_ahead: 2.0,
        }
    }
}

/// What `sync` read from the outside world this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// A session of this exercise is running (the engine and the worker agree).
    pub running: bool,
    /// The audio output runs, so a session can start.
    pub clock_running: bool,
    /// Transport time, in seconds.
    pub clock_now: f64,
    pub view: ViewPrefs,
    /// Reference tuning, in Hz.
    pub a4: f32,
    pub scoring: ScoringThresholds,
    /// Most audio a session's recording keeps.
    pub max_recording_bytes: usize,
    /// The user's own scales (Basic scales).
    pub library: Vec<CustomScale>,
}

#[derive(Default)]
pub struct Model {
    pub kind: Kind,
    pub data: BasicTimingData,
    /// Written by the session on the worker thread; read through `lock`.
    pub shared: SharedTiming,
    /// Basic scales' targets, written by its session.
    pub scale: SharedScale,
    pub frame: Frame,
    /// A session ran since the last configuration change, so its results are shown.
    pub has_results: bool,
    /// The results dialog is open.
    pub show_results: bool,
    pub capture: SessionCapture,
    /// The Score + Audio tab: a sub-program whose messages and effects this exercise
    /// routes (`Msg::Review`, `Effect::Review`).
    pub review: Option<Review>,
    pub results_tab: ResultsTab,
    /// The toolbar's tap-tempo button (its messages are routed: `Msg::Tap`).
    pub tap: tap_tempo::State,
}

impl Model {
    /// Basic scales' settings; `None` in Basic timing.
    pub fn scales(&self) -> Option<ScaleSettings> {
        (self.kind == Kind::Scales).then(|| self.data.scales.clone().unwrap_or_default())
    }

    /// The notes Basic scales plays in turn.
    pub fn sequence(&self) -> Option<Vec<u8>> {
        let s = self.scales()?;
        Some(s.sequence(&self.data.session.key, &self.frame.library))
    }
}
