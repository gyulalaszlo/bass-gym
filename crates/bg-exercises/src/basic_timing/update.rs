//! How Basic timing's model changes. Pure: no egui and no engine.

use bg_ui::kit::Emit;
use bg_ui::views::results::ResultsTab;
use bg_ui::views::review::{self, Review};

use super::effects::{Effect, Start};
use super::model::Model;
use super::msg::Msg;

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
        Msg::ReviewReady(input) => {
            if m.has_results && m.review.is_none() {
                m.review = Some(Review::new(*input));
            }
        }
        Msg::Review(msg) => {
            if let Some(r) = m.review.as_mut() {
                let mut review_fx = Emit::new();
                review::update(r, msg, &mut review_fx);
                fx.extend_map(review_fx, Effect::Review);
            }
        }
        Msg::Transport => {
            if m.frame.running {
                fx.send(Effect::Stop);
            } else if m.frame.clock_running {
                fx.send(Effect::Start(Box::new(Start {
                    cfg: m.data.session.clone(),
                    scoring: m.frame.scoring.clone(),
                    shared: m.shared.clone(),
                    max_recording_bytes: m.frame.max_recording_bytes,
                })));
            }
        }
        Msg::Started(id) => {
            m.capture.begin(id);
            m.review = None;
            m.results_tab = ResultsTab::Stats;
            m.has_results = true;
            m.show_results = false;
        }
        Msg::OpenResults => {
            if m.has_results && !m.frame.running {
                m.show_results = true;
            }
        }
        Msg::Session(cfg) => {
            if *cfg != m.data.session {
                m.data.session = *cfg;
                m.has_results = false;
            }
        }
        Msg::ResultsOpen(open) => m.show_results = open && m.has_results,
        Msg::ResultsTab(tab) => m.results_tab = tab,
        Msg::Load(data) => {
            m.data = *data;
            m.has_results = false;
        }
        Msg::FakeSession {
            recording,
            open_results,
            open_stats,
        } => {
            m.capture.set(recording);
            m.has_results = true;
            m.show_results = open_results;
            m.results_tab = if open_stats {
                ResultsTab::Stats
            } else {
                ResultsTab::Review
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic_timing::model::Frame;

    fn send(m: &mut Model, msg: Msg) -> Vec<Effect> {
        let mut fx = Emit::new();
        update(m, msg, &mut fx);
        fx.into_vec()
    }

    fn frame(running: bool, clock_running: bool) -> Msg {
        Msg::Frame(Frame {
            running,
            clock_running,
            ..Frame::default()
        })
    }

    #[test]
    fn transport_starts_only_with_audio_output() {
        let mut m = Model::default();
        send(&mut m, frame(false, false));
        assert!(send(&mut m, Msg::Transport).is_empty());
        send(&mut m, frame(false, true));
        let fx = send(&mut m, Msg::Transport);
        assert!(matches!(fx.as_slice(), [Effect::Start(_)]));
    }

    #[test]
    fn transport_stops_a_running_session() {
        let mut m = Model::default();
        send(&mut m, frame(true, true));
        assert!(matches!(
            send(&mut m, Msg::Transport).as_slice(),
            [Effect::Stop]
        ));
    }

    #[test]
    fn the_end_of_a_session_opens_its_results() {
        let mut m = Model::default();
        send(&mut m, frame(false, true));
        send(&mut m, Msg::Started(bg_audio::SessionId::UNUSED));
        assert!(m.has_results && !m.show_results);
        send(&mut m, frame(true, true));
        assert!(!m.show_results);
        send(&mut m, frame(false, true));
        assert!(m.show_results);
    }

    #[test]
    fn editing_the_session_drops_the_results() {
        let mut m = Model::default();
        send(&mut m, Msg::Started(bg_audio::SessionId::UNUSED));
        let same = Box::new(m.data.session.clone());
        send(&mut m, Msg::Session(same));
        assert!(m.has_results, "an unchanged config keeps the results");
        let mut cfg = m.data.session.clone();
        cfg.tempo += 10.0;
        send(&mut m, Msg::Session(Box::new(cfg)));
        assert!(!m.has_results);
        send(&mut m, Msg::ResultsOpen(true));
        assert!(!m.show_results, "no results to show");
    }

    #[test]
    fn results_open_only_after_a_session() {
        let mut m = Model::default();
        send(&mut m, Msg::OpenResults);
        assert!(!m.show_results);
        send(&mut m, Msg::Started(bg_audio::SessionId::UNUSED));
        send(&mut m, frame(true, true));
        send(&mut m, Msg::OpenResults);
        assert!(!m.show_results, "not while running");
        send(&mut m, frame(false, true));
        send(&mut m, Msg::ResultsOpen(false));
        send(&mut m, Msg::OpenResults);
        assert!(m.show_results);
    }
}
