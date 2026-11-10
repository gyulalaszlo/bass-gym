//! Everything that can happen to Basic timing.

use std::sync::Arc;

use bg_audio::{Recording, SessionId};
use bg_core::SessionConfig;
use bg_ui::widgets::results::ResultsTab;
use bg_ui::widgets::session_review::ReviewInput;

use super::model::{BasicTimingData, Frame};

pub enum Msg {
    // From `sync`: the outside world.
    /// This frame's reading of the engine and settings.
    Frame(Frame),
    /// The latest session's recording arrived.
    Recorded(Arc<Recording>),
    /// What the review needs, once the session has fully ended.
    Review(Box<ReviewInput>),

    // From the views.
    /// Start / stop was pressed.
    Transport,
    OpenResults,
    /// The session settings were edited.
    Session(Box<SessionConfig>),
    /// The results dialog opened or closed itself.
    ResultsOpen(bool),
    ResultsTab(ResultsTab),

    // From effects.
    Started(SessionId),

    // From the shell.
    Load(Box<BasicTimingData>),
    /// A made-up session ran offline (debugging aid).
    FakeSession {
        recording: Arc<Recording>,
        open_results: bool,
    },
}
