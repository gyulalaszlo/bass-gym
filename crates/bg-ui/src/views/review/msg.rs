//! Everything that can happen to the review.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use bg_audio::ReplayHandle;
use bg_report::Exported;

use crate::widgets::marker_editor::{EditorState, Marker};

pub enum Msg {
    // From `sync` and the owner.
    /// Transport time, and whether any session (a replay included) is running.
    Clock {
        now: f64,
        session_running: bool,
    },
    /// After the dialog drew, while a replay runs: ends it when the dialog closed or
    /// playback finished.
    Tick {
        dialog_open: bool,
    },
    ExportDone(Result<Exported, String>),

    // From the view.
    /// Play / stop the selected bars (the button, or Space).
    TogglePlay,
    /// Play from here to the end of the session (a click on the score).
    PlayFrom(f64),
    Click(bool),
    Chords(bool),
    Log(bool),
    FromBar(i64),
    ToBar(i64),
    /// A click on bar `bar` (from 1); with `extend`, Shift was held.
    SelectBar {
        bar: i64,
        extend: bool,
    },
    OpenEditor(f64),
    BackToScore,
    /// The score scrolled to the time asked for.
    Revealed,
    /// The marker editor's markers and state after this frame (it edits in place, not
    /// migrated yet: it gets copies).
    Editor(Box<EditorChange>),
    ReportTitle(String),
    ReportNotes(String),
    /// Export the selected bars as a report into this data folder.
    Export(PathBuf),
    /// The export folder's path was copied.
    Copied,
    OpenFolder,

    // From effects.
    ReplayStarted(ReplayHandle),
    ExportRunning(Receiver<Result<Exported, String>>),
}

#[derive(Clone, PartialEq)]
pub struct EditorChange {
    pub markers: Vec<Marker>,
    pub state: EditorState,
    pub next_marker: u64,
}
