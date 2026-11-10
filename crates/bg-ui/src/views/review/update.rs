//! How the review changes. Pure: no egui and no engine.

use bg_audio::ReplayHandle;

use super::effects::Effect;
use super::model::{Export, Review};
use super::msg::Msg;
use crate::kit::Emit;

/// Starts a replay of `from..to` with the current options.
fn start(r: &Review, from: f64, to: f64, fx: &mut Emit<Effect>) {
    fx.send(Effect::StartReplay {
        recording: r.input.recording.clone(),
        from,
        to,
        handle: ReplayHandle::new(r.click, r.chords),
    });
}

pub fn update(r: &mut Review, msg: Msg, fx: &mut Emit<Effect>) {
    match msg {
        Msg::Clock {
            now,
            session_running,
        } => {
            r.now = now;
            r.session_running = session_running;
        }
        Msg::Tick { dialog_open } => {
            if r.replay.is_none() {
                return;
            }
            if !dialog_open {
                r.queued = None;
                if r.session_running {
                    fx.send(Effect::Stop);
                }
            }
            if !r.session_running {
                fx.send(Effect::Release);
                r.replay = None;
                if let Some((from, to)) = r.queued.take() {
                    start(r, from, to, fx);
                }
            }
        }
        Msg::ExportDone(result) => {
            r.export = match result {
                Ok(out) => Export::Done {
                    folder: out.folder,
                    copied: false,
                },
                Err(e) => Export::Failed(e),
            };
        }
        Msg::TogglePlay => {
            r.queued = None;
            if r.is_playing() {
                fx.send(Effect::Stop);
            } else if let Some((from, to)) = r.selected_range() {
                start(r, from, to, fx);
            }
        }
        Msg::PlayFrom(from) => {
            let Some(to) = r.score.last().map(|b| b.end) else {
                return;
            };
            if r.is_playing() {
                // It takes the worker a moment to stop: `Tick` starts this one after that.
                r.queued = Some((from, to));
                fx.send(Effect::Stop);
            } else {
                start(r, from, to, fx);
            }
        }
        Msg::Click(on) => {
            r.click = on;
            set_replay(r, fx);
        }
        Msg::Chords(on) => {
            r.chords = on;
            set_replay(r, fx);
        }
        Msg::Log(on) => r.log = on,
        Msg::FromBar(bar) => {
            r.from_bar = bar;
            r.to_bar = r.to_bar.max(r.from_bar);
        }
        Msg::ToBar(bar) => r.to_bar = bar.max(r.from_bar),
        Msg::SelectBar { bar, extend } => {
            if extend {
                r.from_bar = r.from_bar.min(bar);
                r.to_bar = r.to_bar.max(bar);
            } else {
                r.from_bar = bar;
                r.to_bar = bar;
            }
        }
        Msg::OpenEditor(t) => r.open_editor(t),
        Msg::BackToScore => {
            if let Some(editor) = r.editor.take() {
                r.reveal = Some(editor.view.start + editor.view.span / 2.0);
            }
        }
        Msg::Revealed => r.reveal = None,
        Msg::Editor(change) => {
            if r.editor.is_some() {
                r.markers = change.markers;
                r.editor = Some(change.state);
                r.next_marker = change.next_marker;
            }
        }
        Msg::ReportTitle(t) => r.report_title = t,
        Msg::ReportNotes(t) => r.report_notes = t,
        Msg::Export(data_dir) => {
            if let Some(input) = r.report_input() {
                fx.send(Effect::Export {
                    input: Box::new(input),
                    reports: data_dir.join(bg_report::REPORTS_FOLDER),
                });
            }
        }
        Msg::Copied => {
            if let Export::Done { copied, .. } = &mut r.export {
                *copied = true;
            }
        }
        Msg::OpenFolder => {
            if let Export::Done { folder, .. } = &r.export {
                fx.send(Effect::OpenFolder(folder.clone()));
            }
        }
        Msg::ReplayStarted(handle) => r.replay = Some(handle),
        Msg::ExportRunning(rx) => r.export = Export::Running(rx),
    }
}

/// Passes the click / chords options to a running replay.
fn set_replay(r: &Review, fx: &mut Emit<Effect>) {
    if let Some(handle) = &r.replay {
        fx.send(Effect::SetReplay {
            handle: handle.clone(),
            click: r.click,
            chords: r.chords,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::review::ReviewInput;
    use bg_audio::Recorder;
    use bg_core::TimeSignature;
    use bg_core::score::ScoreBar;
    use std::sync::Arc;

    fn bar(index: i64, start: f64) -> ScoreBar {
        ScoreBar {
            index,
            start,
            end: start + 2.0,
            bpm: 120.0,
            show_bpm: false,
            sig: TimeSignature::default(),
            events: Vec::new(),
        }
    }

    /// A review of three bars, 10 s to 16 s, all selected.
    fn review() -> Review {
        let mut r = Review::new(ReviewInput {
            bars: Vec::new(),
            matcher: Default::default(),
            chords: Vec::new(),
            scale: None,
            a4: 440.0,
            recording: Arc::new(Recorder::new(1024).finish(0.0, 0.0)),
            exercise: "test".into(),
            settings: serde_json::Value::Null,
        });
        r.score = vec![bar(0, 10.0), bar(1, 12.0), bar(2, 14.0)];
        (r.from_bar, r.to_bar) = (1, 3);
        r
    }

    fn send(r: &mut Review, msg: Msg) -> Vec<Effect> {
        let mut fx = Emit::new();
        update(r, msg, &mut fx);
        fx.into_vec()
    }

    fn playing(r: &mut Review) {
        r.replay = Some(ReplayHandle::new(true, true));
        send(
            r,
            Msg::Clock {
                now: 0.0,
                session_running: true,
            },
        );
    }

    #[test]
    fn play_starts_the_selected_bars() {
        let mut r = review();
        send(
            &mut r,
            Msg::SelectBar {
                bar: 2,
                extend: false,
            },
        );
        match send(&mut r, Msg::TogglePlay).as_slice() {
            [Effect::StartReplay { from, to, .. }] => assert_eq!((*from, *to), (12.0, 14.0)),
            _ => panic!("expected a replay"),
        }
    }

    #[test]
    fn playing_from_a_click_waits_for_the_running_replay_to_stop() {
        let mut r = review();
        playing(&mut r);
        assert!(matches!(
            send(&mut r, Msg::PlayFrom(13.0)).as_slice(),
            [Effect::Stop]
        ));
        assert_eq!(r.queued, Some((13.0, 16.0)));
        // Still stopping: nothing happens yet.
        assert!(send(&mut r, Msg::Tick { dialog_open: true }).is_empty());
        send(
            &mut r,
            Msg::Clock {
                now: 0.0,
                session_running: false,
            },
        );
        let fx = send(&mut r, Msg::Tick { dialog_open: true });
        assert!(matches!(
            fx.as_slice(),
            [Effect::Release, Effect::StartReplay { from, .. }] if *from == 13.0
        ));
        assert!(r.replay.is_none(), "until the engine confirms");
    }

    #[test]
    fn closing_the_dialog_stops_playback_and_drops_the_queue() {
        let mut r = review();
        playing(&mut r);
        r.queued = Some((12.0, 16.0));
        let fx = send(&mut r, Msg::Tick { dialog_open: false });
        assert!(matches!(fx.as_slice(), [Effect::Stop]));
        assert_eq!(r.queued, None);
    }

    #[test]
    fn shift_extends_the_selection_and_the_end_never_precedes_the_start() {
        let mut r = review();
        send(
            &mut r,
            Msg::SelectBar {
                bar: 2,
                extend: false,
            },
        );
        send(
            &mut r,
            Msg::SelectBar {
                bar: 3,
                extend: true,
            },
        );
        assert_eq!((r.from_bar, r.to_bar), (2, 3));
        send(&mut r, Msg::ToBar(1));
        assert_eq!(r.to_bar, 2);
    }

    #[test]
    fn options_reach_a_running_replay() {
        let mut r = review();
        assert!(send(&mut r, Msg::Click(false)).is_empty());
        playing(&mut r);
        assert!(matches!(
            send(&mut r, Msg::Chords(false)).as_slice(),
            [Effect::SetReplay {
                click: false,
                chords: false,
                ..
            }]
        ));
    }
}
