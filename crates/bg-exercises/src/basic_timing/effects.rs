//! What Basic timing asks of the engine. The only code here that drives it.

use bg_audio::AudioEngine;
use bg_core::{ScoringThresholds, SessionConfig};
use bg_ui::kit::Emit;
use bg_ui::views::review;

use super::msg::Msg;
use crate::basic_scales::{ScaleTargets, SharedScale};
use crate::timing_session::{AccompanimentScoring, SharedTiming, TimingSession};

pub enum Effect {
    Start(Box<Start>),
    Stop,
    /// The review's own effects, routed here.
    Review(review::Effect),
}

/// A session to start, recorded.
pub struct Start {
    pub cfg: SessionConfig,
    pub scoring: ScoringThresholds,
    pub shared: SharedTiming,
    pub max_recording_bytes: usize,
    pub scale: Option<ScaleStart>,
}

/// Basic scales' part of a start: where its targets go, the notes and the reference
/// tuning to judge by.
pub struct ScaleStart {
    pub shared: SharedScale,
    pub seq: Vec<u8>,
    pub a4: f32,
}

pub fn run(effect: Effect, engine: &mut AudioEngine, out: &mut Emit<Msg>) {
    match effect {
        Effect::Start(s) => {
            let logic = match s.scale {
                Some(sc) => {
                    let targets = ScaleTargets::new(sc.seq, sc.a4, sc.shared);
                    TimingSession::with_accompaniment(
                        s.cfg,
                        s.scoring,
                        s.shared,
                        Box::new(targets),
                        AccompanimentScoring::default(),
                    )
                }
                None => TimingSession::new(s.cfg, s.scoring, s.shared),
            };
            let id = engine.start_recorded(Box::new(logic), s.max_recording_bytes);
            out.send(Msg::Started(id));
        }
        Effect::Stop => engine.stop_session(),
        Effect::Review(e) => {
            let mut review_out = Emit::new();
            review::run(e, engine, &mut review_out);
            out.extend_map(review_out, Msg::Review);
        }
    }
}
