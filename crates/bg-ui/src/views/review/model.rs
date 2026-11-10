//! What the review remembers: the session it shows, the selection, playback, markers,
//! the marker editor and the report being written.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use bg_audio::{Recording, ReplayHandle};
use bg_core::score::{self, ScoreBar, ScoreInput};
use bg_report::{Annotation, Exported, ReportInput};

use super::{ReviewInput, overlays};
use crate::theme;
use crate::widgets::marker_editor::{EditorState, Line, Marker, WaveView};
use crate::widgets::score::StaffLayout;

/// Seconds shown either side of a click when opening the marker editor.
const EDITOR_SPAN: f64 = 1.0;

/// How the latest export went.
pub(crate) enum Export {
    Idle,
    Running(Receiver<Result<Exported, String>>),
    Done { folder: PathBuf, copied: bool },
    Failed(String),
}

pub struct Review {
    pub(crate) input: ReviewInput,
    pub(crate) score: Vec<ScoreBar>,
    /// Expected, played and bar markers: drawn, not editable.
    pub(crate) overlays: Vec<Marker>,
    /// Staff range; the owner sets the spacing each frame (see `view::Props`).
    pub(crate) layout: StaffLayout,
    /// First and last selected bar, numbered from 1.
    pub from_bar: i64,
    pub to_bar: i64,
    pub(crate) click: bool,
    pub(crate) chords: bool,
    pub(crate) log: bool,
    /// The running replay.
    pub(crate) replay: Option<ReplayHandle>,
    /// What to play once the running replay has stopped.
    pub(crate) queued: Option<(f64, f64)>,
    /// Annotations, in transport time.
    pub markers: Vec<Marker>,
    pub(crate) next_marker: u64,
    pub(crate) editor: Option<EditorState>,
    /// Scroll the score to show this time (set when leaving the marker editor).
    pub(crate) reveal: Option<f64>,
    pub(crate) report_title: String,
    pub(crate) report_notes: String,
    pub(crate) export: Export,
    /// From the last [`super::sync`]: transport time, and whether any session (a replay
    /// included) is running.
    pub(crate) now: f64,
    pub(crate) session_running: bool,
}

impl Review {
    pub fn new(input: ReviewInput) -> Self {
        let score = score::build(&ScoreInput {
            bars: &input.bars,
            matcher: &input.matcher,
            chords: &input.chords,
            scale: input.scale,
            a4: input.a4,
            until: f64::INFINITY,
        });
        let (lo, hi) = score::extent(&score);
        let overlays = overlays(&input.matcher, &score);
        let (from_bar, to_bar) = match (score.first(), score.last()) {
            (Some(a), Some(b)) => (a.index + 1, b.index + 1),
            _ => (1, 1),
        };
        let mut review = Self {
            score,
            overlays,
            layout: StaffLayout {
                spacing: 8.0,
                lo,
                hi,
                labels: true,
            },
            from_bar,
            to_bar,
            click: true,
            chords: true,
            log: false,
            replay: None,
            queued: None,
            markers: Vec::new(),
            next_marker: 1 << 32,
            editor: None,
            reveal: None,
            report_title: String::new(),
            report_notes: String::new(),
            export: Export::Idle,
            now: 0.0,
            session_running: false,
            input,
        };
        review.debug_open_editor();
        review
    }

    /// Debugging aid: `BASS_GYM_FAKE_SESSION=editor` opens the marker editor with a marker.
    fn debug_open_editor(&mut self) {
        if std::env::var("BASS_GYM_FAKE_SESSION").as_deref() != Ok("editor") {
            return;
        }
        let Some(note) = self.input.matcher.played.get(4) else {
            return;
        };
        let t = note.onset + 0.2;
        self.open_editor(t);
        if let Some(editor) = self.editor.as_mut() {
            editor.selected = Some(self.next_marker);
        }
        self.markers.push(Marker {
            id: self.next_marker,
            time: t,
            end: None,
            label: "note end should be detected here, and this is a long note to show \
                    how the label grows with what is typed into it"
                .into(),
            color: theme::cyan(),
            line: Line::Solid,
        });
    }

    pub fn recording(&self) -> &Arc<Recording> {
        &self.input.recording
    }

    /// Transport time of the selected bars.
    pub fn selected_range(&self) -> Option<(f64, f64)> {
        let selected = |b: &&ScoreBar| (self.from_bar - 1..self.to_bar).contains(&b.index);
        let first = self.score.iter().find(selected)?;
        let last = self.score.iter().rev().find(selected)?;
        Some((first.start, last.end))
    }

    /// A replay is running.
    pub fn is_playing(&self) -> bool {
        self.replay.is_some()
    }

    /// Where the replay is now, in transport time.
    pub(crate) fn playhead(&self) -> Option<f64> {
        self.replay.as_ref()?.position(self.now)
    }

    /// Play / stop can be used: not while another session runs.
    pub(crate) fn can_play(&self) -> bool {
        self.is_playing() || !self.session_running
    }

    /// The first and last bar, numbered from 1.
    pub(crate) fn bar_range(&self) -> (i64, i64) {
        match (self.score.first(), self.score.last()) {
            (Some(a), Some(b)) => (a.index + 1, b.index + 1),
            _ => (1, 1),
        }
    }

    pub(crate) fn open_editor(&mut self, t: f64) {
        let rec = &self.input.recording;
        if !rec.has_audio() {
            return;
        }
        let bounds = (rec.first_time(), rec.end_time());
        let view = WaveView::new(t - EDITOR_SPAN, t + EDITOR_SPAN, bounds);
        let mut state = EditorState::new(view);
        state.log = self.log;
        self.editor = Some(state);
    }

    /// What a report of the selected bars is made from.
    pub(crate) fn report_input(&self) -> Option<ReportInput> {
        let (from, to) = self.selected_range()?;
        let i = &self.input;
        Some(ReportInput {
            title: self.report_title.clone(),
            notes: self.report_notes.clone(),
            exercise: i.exercise.clone(),
            settings: i.settings.clone(),
            a4: i.a4,
            recording: i.recording.clone(),
            bars: i.bars.clone(),
            matcher: i.matcher.clone(),
            chords: i.chords.clone(),
            annotations: self
                .markers
                .iter()
                .map(|m| Annotation {
                    time: m.time,
                    text: m.label.clone(),
                })
                .collect(),
            first_bar: self.from_bar,
            last_bar: self.to_bar,
            from,
            to,
        })
    }
}
