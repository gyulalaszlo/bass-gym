//! How Basic chords' model changes. Pure: no egui and no engine.

use bg_ui::kit::Emit;
use bg_ui::widgets::results::ResultsTab;
use bg_ui::widgets::session_review::SessionReview;

use super::data::{BasicChordsData, resolve_progression};
use super::effects::{Effect, Start};
use super::model::Model;
use super::msg::Msg;
use super::view::config::{piano, progression, scoring};

/// What a session started now would be, or why it can't start.
pub fn start(m: &Model) -> Result<Start, String> {
    let seed = if m.data.new_seed_each_start {
        m.frame.wall_seed
    } else {
        m.data.seed
    };
    let progression = resolve_progression(&m.data, seed)?;
    Ok(Start {
        data: m.data.clone(),
        progression,
        seed,
        a4: m.frame.a4,
        grade_mode: m.frame.grade_mode,
        scoring: m.frame.scoring.clone(),
        timing: m.timing.clone(),
        chords: m.chords.clone(),
        max_recording_bytes: m.frame.max_recording_bytes,
    })
}

/// Applies a settings change. A real change drops the results.
fn edit(m: &mut Model, f: impl FnOnce(&mut BasicChordsData)) {
    let before = m.data.clone();
    f(&mut m.data);
    if m.data != before {
        m.has_results = false;
        m.run_seed = None;
    }
}

pub fn update(m: &mut Model, msg: Msg, fx: &mut Emit<Effect>) {
    match msg {
        Msg::Frame(f) => {
            // The session just ended: show its results.
            if m.frame.running && !f.running && m.has_results {
                m.show_results = true;
            }
            m.frame = f;
            // Results are gone (config changed): free the recording and the review.
            if !m.has_results {
                m.capture.clear();
                m.review = None;
            }
        }
        Msg::Recorded(r) => {
            if m.has_results {
                m.capture.set(r);
            }
        }
        Msg::Review(input) => {
            if m.has_results && m.review.is_none() {
                m.review = Some(SessionReview::new(*input));
            }
        }
        Msg::Transport => {
            if m.frame.running {
                fx.send(Effect::Stop);
            } else if m.frame.clock_running {
                match start(m) {
                    Ok(s) => {
                        m.error = None;
                        fx.send(Effect::Start(Box::new(s)));
                    }
                    Err(e) => m.error = Some(format!("Can't start: {e}")),
                }
            }
        }
        Msg::Started { id, seed } => {
            m.capture.begin(id);
            m.review = None;
            m.results_tab = ResultsTab::Stats;
            m.has_results = true;
            m.show_results = false;
            m.run_seed = Some(seed);
        }
        Msg::OpenResults => {
            if m.has_results && !m.frame.running {
                m.show_results = true;
            }
        }
        Msg::Session(cfg) => edit(m, |d| d.session = *cfg),
        Msg::Progression(e) => edit(m, |d| match e {
            progression::Emission::Progression(p) => d.progression = p,
            progression::Emission::ChordLength(l) => d.chord_length = l,
            progression::Emission::Seed(s) => d.seed = s,
            progression::Emission::NewSeedEachStart(b) => d.new_seed_each_start = b,
            progression::Emission::KeepRunSeed(s) => {
                d.seed = s;
                d.new_seed_each_start = false;
            }
        }),
        Msg::Piano(e) => edit(m, |d| match e {
            piano::Emission::Comp(c) => d.comp = c,
            piano::Emission::Jazziness(v) => d.jazziness = v,
            piano::Emission::Width(v) => d.width = v,
            piano::Emission::CenterOctave(o) => d.center_octave = o,
            piano::Emission::Humanize(v) => d.humanize = v,
            piano::Emission::PianoVolume(v) => d.piano_volume = v,
            piano::Emission::Release(v) => d.release = v,
            piano::Emission::Keyboard(k) => d.keyboard = k,
        }),
        Msg::Scoring(e) => edit(m, |d| match e {
            scoring::Emission::Scoring(s) => d.scoring = s,
            scoring::Emission::Checker(c) => d.checker = c,
        }),
        Msg::ResultsOpen(open) => m.show_results = open && m.has_results,
        Msg::ResultsTab(tab) => m.results_tab = tab,
        Msg::Load(data) => {
            m.data = *data;
            m.has_results = false;
            m.run_seed = None;
        }
        Msg::FakeSession {
            recording,
            open_results,
            seed,
        } => {
            m.capture.set(recording);
            m.has_results = true;
            m.show_results = open_results;
            m.results_tab = ResultsTab::Review;
            m.run_seed = Some(seed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic_chords::data::{FixedSource, ProgressionKind};
    use crate::basic_chords::model::Frame;
    use bg_audio::SessionId;

    fn send(m: &mut Model, msg: Msg) -> Vec<Effect> {
        let mut fx = Emit::new();
        update(m, msg, &mut fx);
        fx.into_vec()
    }

    fn ready() -> Model {
        let mut m = Model::default();
        send(
            &mut m,
            Msg::Frame(Frame {
                clock_running: true,
                wall_seed: 99,
                ..Frame::default()
            }),
        );
        m
    }

    #[test]
    fn starts_with_the_preset_seed_or_a_new_one() {
        let mut m = ready();
        m.data.seed = 7;
        let Ok(s) = start(&m) else { panic!() };
        assert_eq!(s.seed, 7);
        m.data.new_seed_each_start = true;
        let Ok(s) = start(&m) else { panic!() };
        assert_eq!(s.seed, 99);
    }

    #[test]
    fn a_bad_progression_reports_instead_of_starting() {
        let mut m = ready();
        m.data.progression.kind = ProgressionKind::Fixed;
        m.data.progression.source = FixedSource::Custom;
        m.data.progression.custom = "not numerals".into();
        assert!(send(&mut m, Msg::Transport).is_empty());
        assert!(
            m.error
                .as_deref()
                .is_some_and(|e| e.starts_with("Can't start"))
        );
        m.data.progression.custom = "I IV V".into();
        assert!(matches!(
            send(&mut m, Msg::Transport).as_slice(),
            [Effect::Start(_)]
        ));
        assert_eq!(m.error, None);
    }

    #[test]
    fn edits_drop_the_results_and_the_run_seed() {
        let mut m = ready();
        send(
            &mut m,
            Msg::Started {
                id: SessionId::UNUSED,
                seed: 5,
            },
        );
        assert_eq!(m.run_seed, Some(5));
        let same = m.data.jazziness;
        send(&mut m, Msg::Piano(piano::Emission::Jazziness(same)));
        assert!(m.has_results, "no change, nothing dropped");
        send(&mut m, Msg::Piano(piano::Emission::Jazziness(0.9)));
        assert!(!m.has_results);
        assert_eq!(m.run_seed, None);
    }

    #[test]
    fn keeping_the_run_seed_turns_off_new_seeds() {
        let mut m = ready();
        m.data.new_seed_each_start = true;
        send(
            &mut m,
            Msg::Progression(progression::Emission::KeepRunSeed(42)),
        );
        assert_eq!((m.data.seed, m.data.new_seed_each_start), (42, false));
    }
}
