//! Reading the outside world (engine, worker state, settings) into messages, once a
//! frame. The only part of Basic chords that reads it.

use bg_core::score::ScoreChord;
use bg_ui::ExerciseCtx;
use bg_ui::recording::RecordingPrefs;
use bg_ui::view::ViewPrefs;
use bg_ui::widgets::session_review::{ReviewInput, report_settings};
use bg_ui::widgets::tuner::TunerPrefs;

use super::ID;
use super::accompaniment::lock_chords;
use super::model::{Frame, Model};
use super::msg::Msg;
use super::prefs::BasicChordsPrefs;
use crate::timing_session::lock;

/// This frame's reading, and the recording if it just arrived.
pub fn sync(m: &Model, ctx: &ExerciseCtx) -> Vec<Msg> {
    let engine = &*ctx.engine;
    let clock = engine.clock();
    let wall_seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let mut msgs = vec![Msg::Frame(Frame {
        running: engine.session_running() && lock(&m.timing).running,
        clock_running: clock.is_running(),
        clock_now: clock.now(),
        view: ctx.settings.get(ViewPrefs::ID),
        a4: ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4,
        scoring: ctx.scoring.clone(),
        grade_mode: ctx
            .settings
            .get::<BasicChordsPrefs>(BasicChordsPrefs::ID)
            .grade_mode,
        max_recording_bytes: ctx
            .settings
            .get::<RecordingPrefs>(RecordingPrefs::ID)
            .max_bytes(),
        wall_seed,
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
    let recording = m.capture.recording()?.clone();
    // One lock at a time: chords first, then timing.
    let chords: Vec<ScoreChord> = lock_chords(&m.chords)
        .spans
        .iter()
        .map(|s| ScoreChord {
            start: s.start,
            end: s.end,
            tones: s.chord.tones(),
            label: m.data.chord_label(&s.chord),
        })
        .collect();
    let preset = serde_json::to_value(&m.data).unwrap_or_default();
    let settings = report_settings(ctx, preset);
    let shared = lock(&m.timing);
    Some(Msg::Review(Box::new(ReviewInput {
        bars: shared.bars.clone(),
        matcher: shared.matcher.clone(),
        chords,
        scale: Some(m.data.session.key),
        a4: m.frame.a4,
        recording,
        exercise: ID.to_owned(),
        settings,
    })))
}
