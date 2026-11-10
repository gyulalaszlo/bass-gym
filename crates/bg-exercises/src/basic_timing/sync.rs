//! Reading the outside world (engine, worker state, settings) into messages, once a
//! frame. The only part of Basic timing that reads it.

use bg_ui::ExerciseCtx;
use bg_ui::recording::RecordingPrefs;
use bg_ui::view::ViewPrefs;
use bg_ui::views::review::{self, ReviewInput, report_settings};
use bg_ui::widgets::tuner::TunerPrefs;

use super::ID;
use super::model::{Frame, Model};
use super::msg::Msg;
use crate::timing_session::lock;

/// This frame's reading, and the recording if it just arrived.
pub fn sync(m: &Model, ctx: &ExerciseCtx) -> Vec<Msg> {
    let engine = &*ctx.engine;
    let clock = engine.clock();
    let mut msgs = vec![Msg::Frame(Frame {
        running: engine.session_running() && lock(&m.shared).running,
        clock_running: clock.is_running(),
        clock_now: clock.now(),
        view: ctx.settings.get(ViewPrefs::ID),
        a4: ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4,
        scoring: ctx.scoring.clone(),
        max_recording_bytes: ctx
            .settings
            .get::<RecordingPrefs>(RecordingPrefs::ID)
            .max_bytes(),
    })];
    if let Some(id) = m.capture.pending()
        && let Some(r) = engine.take_recording(id)
    {
        msgs.push(Msg::Recorded(r));
    }
    msgs
}

/// What the review needs, once the session has fully ended and its recording is in.
/// Runs after [`sync`]'s messages are handled, so it sees their outcome.
pub fn review(m: &Model, ctx: &ExerciseCtx) -> Option<Msg> {
    if m.review.is_some() || !m.has_results || ctx.engine.session_running() {
        return None;
    }
    let recording = m.capture.recording()?;
    let preset = serde_json::to_value(&m.data).unwrap_or_default();
    let settings = report_settings(ctx, preset);
    let shared = lock(&m.shared);
    Some(Msg::ReviewReady(Box::new(ReviewInput {
        bars: shared.bars.clone(),
        matcher: shared.matcher.clone(),
        chords: Vec::new(),
        scale: Some(m.data.session.key),
        a4: m.frame.a4,
        recording: recording.clone(),
        exercise: ID.to_owned(),
        settings,
    })))
}

/// The review's own reading of the engine, before it shows.
pub fn review_sync(m: &Model, ctx: &ExerciseCtx) -> Vec<Msg> {
    m.review.as_ref().map_or_else(Vec::new, |r| {
        review::sync(r, ctx.engine)
            .into_iter()
            .map(Msg::Review)
            .collect()
    })
}
