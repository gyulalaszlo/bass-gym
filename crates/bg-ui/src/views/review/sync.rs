//! Reading the engine and the export thread into messages, before the review shows.

use std::sync::mpsc::TryRecvError;

use bg_audio::AudioEngine;

use super::model::{Export, Review};
use super::msg::Msg;

/// Only sends what changed, so an idle review doesn't keep the app repainting: the
/// clock only while a replay runs (for the playhead).
pub fn sync(r: &Review, engine: &AudioEngine) -> Vec<Msg> {
    let mut msgs = Vec::new();
    let session_running = engine.session_running();
    if r.is_playing() || session_running != r.session_running {
        msgs.push(Msg::Clock {
            now: engine.clock().now(),
            session_running,
        });
    }
    if let Export::Running(rx) = &r.export {
        match rx.try_recv() {
            Ok(result) => msgs.push(Msg::ExportDone(result)),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => msgs.push(Msg::ExportDone(Err(
                "the export stopped unexpectedly".into(),
            ))),
        }
    }
    msgs
}
