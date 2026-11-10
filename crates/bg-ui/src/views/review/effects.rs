//! What the review asks of the engine and the system. The only code here that drives
//! them.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::channel;

use bg_audio::{AudioEngine, Recording, ReplayHandle};
use bg_report::ReportInput;

use super::msg::Msg;
use crate::folder::open_folder;
use crate::kit::Emit;

/// Level of the recorded bass in a replay.
const BASS_GAIN: f32 = 1.0;

pub enum Effect {
    StartReplay {
        recording: Arc<Recording>,
        from: f64,
        to: f64,
        handle: ReplayHandle,
    },
    /// Stops the running session (the replay).
    Stop,
    /// Frees the playback voice once the replay has stopped.
    Release,
    /// What a running replay plays besides the bass.
    SetReplay {
        handle: ReplayHandle,
        click: bool,
        chords: bool,
    },
    /// Writes a report, on a thread of its own: a long session can take a while to zip.
    Export {
        input: Box<ReportInput>,
        reports: PathBuf,
    },
    OpenFolder(PathBuf),
}

pub fn run(effect: Effect, engine: &mut AudioEngine, out: &mut Emit<Msg>) {
    match effect {
        Effect::StartReplay {
            recording,
            from,
            to,
            handle,
        } => {
            if engine.start_replay(&recording, from, to, BASS_GAIN, handle.clone()) {
                out.send(Msg::ReplayStarted(handle));
            }
        }
        Effect::Stop => engine.stop_session(),
        Effect::Release => engine.release_replay(),
        Effect::SetReplay {
            handle,
            click,
            chords,
        } => {
            handle.set_click(click);
            handle.set_chords(chords);
        }
        Effect::Export { input, reports } => {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let result = bg_report::export(&input, &reports).map_err(|e| format!("{e:#}"));
                let _ = tx.send(result);
            });
            out.send(Msg::ExportRunning(rx));
        }
        Effect::OpenFolder(folder) => open_folder(&folder),
    }
}
