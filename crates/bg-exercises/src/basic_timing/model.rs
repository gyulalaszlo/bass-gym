//! What Basic timing knows: its preset data, what it read from the outside world this
//! frame, and the state of its latest session.

use bg_core::{ScoringThresholds, SessionConfig};
use bg_ui::recording::SessionCapture;
use bg_ui::view::ViewPrefs;
use bg_ui::views::results::ResultsTab;
use bg_ui::views::review::Review;
use bg_ui::views::tap_tempo;
use serde::{Deserialize, Serialize};

use crate::timing_session::SharedTiming;

/// The preset data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasicTimingData {
    pub session: SessionConfig,
    pub view: ViewOptions,
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
}

#[derive(Default)]
pub struct Model {
    pub data: BasicTimingData,
    /// Written by the session on the worker thread; read through `lock`.
    pub shared: SharedTiming,
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
