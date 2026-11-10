//! What Basic chords asks of the engine. The only code here that drives it.

use bg_audio::AudioEngine;
use bg_core::harmony::{self, Chord};
use bg_core::{GradeMode, ScoringThresholds};
use bg_ui::kit::Emit;
use bg_ui::views::review;

use super::accompaniment::{ChordAccompaniment, SharedChords};
use super::data::BasicChordsData;
use super::msg::Msg;
use crate::timing_session::{AccompanimentScoring, SharedTiming, TimingSession};

pub enum Effect {
    Start(Box<Start>),
    Stop,
    /// The review's own effects, routed here.
    Review(review::Effect),
}

/// A session to start, recorded: everything [`session`] needs.
pub struct Start {
    pub data: BasicChordsData,
    pub progression: Vec<Chord>,
    pub seed: u64,
    pub a4: f32,
    pub grade_mode: GradeMode,
    pub scoring: ScoringThresholds,
    pub timing: SharedTiming,
    pub chords: SharedChords,
    pub max_recording_bytes: usize,
}

/// The session's logic: the timing session with the comping piano player. Also used to
/// run a made-up session offline.
pub fn session(s: &Start) -> TimingSession {
    let d = &s.data;
    let acc = ChordAccompaniment::new(
        s.progression.clone(),
        d.session.key,
        harmony::checker(&d.checker),
        d.comp.clone(),
        d.chord_length.bars(),
        harmony::VoicingStyle {
            jazziness: d.jazziness,
            width: d.width,
            center_octave: d.center_octave,
        },
        d.piano_volume,
        s.a4,
        s.seed,
        s.chords.clone(),
    )
    .with_humanize(d.humanize)
    .with_release(d.release);
    TimingSession::with_accompaniment(
        d.session.clone(),
        s.scoring.clone(),
        s.timing.clone(),
        Box::new(acc),
        AccompanimentScoring {
            grade_mode: s.grade_mode,
            groove: d.scoring.groove,
            hits: d.scoring.piano,
            harmony: d.scoring.chord_tones,
        },
    )
}

pub fn run(effect: Effect, engine: &mut AudioEngine, out: &mut Emit<Msg>) {
    match effect {
        Effect::Start(s) => {
            let logic = session(&s);
            let id = engine.start_recorded(Box::new(logic), s.max_recording_bytes);
            out.send(Msg::Started { id, seed: s.seed });
        }
        Effect::Stop => engine.stop_session(),
        Effect::Review(e) => {
            let mut review_out = Emit::new();
            review::run(e, engine, &mut review_out);
            out.extend_map(review_out, Msg::Review);
        }
    }
}
