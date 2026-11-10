//! What Basic chords knows: its preset data, what it read from the outside world this
//! frame, and the state of its latest session.

use bg_core::{GradeMode, ScoringThresholds};
use bg_ui::recording::SessionCapture;
use bg_ui::view::ViewPrefs;
use bg_ui::views::results::ResultsTab;
use bg_ui::views::review::Review;

use super::accompaniment::SharedChords;
use super::data::BasicChordsData;
use crate::timing_session::SharedTiming;

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
    /// How chord fit counts in a grade (Basic chords' preferences).
    pub grade_mode: GradeMode,
    /// Most audio a session's recording keeps.
    pub max_recording_bytes: usize,
    /// A seed from the wall clock, for "New each start".
    pub wall_seed: u64,
}

#[derive(Default)]
pub struct Model {
    pub data: BasicChordsData,
    /// Written by the session on the worker thread; read through `lock`.
    pub timing: SharedTiming,
    /// Written by the accompaniment; read through `lock_chords`. Lock it before
    /// `timing` when both are needed, and one at a time where possible.
    pub chords: SharedChords,
    pub frame: Frame,
    /// A session ran since the last configuration change, so its results are shown.
    pub has_results: bool,
    /// The results dialog is open.
    pub show_results: bool,
    /// Seed of the last started session.
    pub run_seed: Option<u64>,
    /// Why the last start failed.
    pub error: Option<String>,
    pub capture: SessionCapture,
    /// The Score + Audio tab: a sub-program whose messages and effects this exercise
    /// routes (`Msg::Review`, `Effect::Review`).
    pub review: Option<Review>,
    pub results_tab: ResultsTab,
}
