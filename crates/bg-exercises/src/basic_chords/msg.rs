//! Everything that can happen to Basic chords.

use std::sync::Arc;

use bg_audio::{Recording, SessionId};
use bg_core::SessionConfig;
use bg_ui::widgets::results::ResultsTab;
use bg_ui::widgets::session_review::ReviewInput;

use super::data::BasicChordsData;
use super::model::Frame;
use super::view::config::{piano, progression, scoring};

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
    Progression(progression::Emission),
    Piano(piano::Emission),
    Scoring(scoring::Emission),
    /// The results dialog opened or closed itself.
    ResultsOpen(bool),
    ResultsTab(ResultsTab),

    // From effects.
    Started {
        id: SessionId,
        seed: u64,
    },

    // From the shell.
    Load(Box<BasicChordsData>),
    /// A made-up session ran offline (debugging aid).
    FakeSession {
        recording: Arc<Recording>,
        open_results: bool,
        seed: u64,
    },
}
