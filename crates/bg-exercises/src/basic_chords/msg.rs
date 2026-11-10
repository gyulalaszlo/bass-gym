//! Everything that can happen to Basic chords.

use std::sync::Arc;

use bg_audio::{Recording, SessionId};
use bg_core::SessionConfig;
use bg_ui::views::results::ResultsTab;
use bg_ui::views::review::{self, ReviewInput};
use bg_ui::views::tap_tempo;

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
    ReviewReady(Box<ReviewInput>),
    /// For the review (its own messages, routed here).
    Review(review::Msg),

    // From the views.
    /// Start / stop was pressed.
    Transport,
    OpenResults,
    /// The session settings were edited.
    Session(Box<SessionConfig>),
    /// For the tap-tempo button (its own messages, routed here).
    Tap(tap_tempo::Msg),
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
        /// On the Statistics tab rather than Score + Audio.
        open_stats: bool,
        seed: u64,
    },
}
