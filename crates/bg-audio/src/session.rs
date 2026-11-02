use bg_sound::SoundEvent;
use rtrb::Producer;

use crate::output::OutMsg;

/// Output voice a sound is played on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Click = 0,
    Groove = 1,
    Instrument = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Finished,
}

/// What a running session can do; handed to [`SessionLogic`] callbacks on the worker thread.
pub struct SessionIo<'a> {
    /// Transport time currently heard.
    pub now: f64,
    /// Round-trip latency already subtracted from note times.
    pub latency: f64,
    pub(crate) out: Option<&'a mut Producer<OutMsg>>,
}

impl SessionIo<'_> {
    /// An I/O handle without audio output, for driving sessions offline (tests, analysis).
    pub fn offline(now: f64, latency: f64) -> SessionIo<'static> {
        SessionIo {
            now,
            latency,
            out: None,
        }
    }

    /// Schedules a sound at transport time `at`. Returns false if output isn't available.
    pub fn play(&mut self, at: f64, slot: Slot, ev: SoundEvent) -> bool {
        self.out
            .as_mut()
            .is_some_and(|p| p.push(OutMsg::Play { at, slot, ev }).is_ok())
    }

    /// Cancels all pending sounds and silences the voices.
    pub fn silence(&mut self) {
        if let Some(p) = self.out.as_mut() {
            let _ = p.push(OutMsg::StopAll);
        }
    }

    pub fn has_output(&self) -> bool {
        self.out.is_some()
    }
}

/// Exercise-specific session behaviour, run on the analysis worker thread. Note times
/// are in transport time with the configured latency already compensated.
pub trait SessionLogic: Send {
    fn start(&mut self, io: &mut SessionIo);
    /// Called every few milliseconds; schedule upcoming sounds here.
    fn tick(&mut self, io: &mut SessionIo) -> SessionState;
    fn note_on(&mut self, time: f64, level_db: f32);
    /// Early pitch estimate of the note that started at `onset`, sent once while it
    /// still sounds (~80–250 ms in). The final pitch comes with `note_off`.
    fn note_pitch(&mut self, onset: f64, freq: f32) {
        let _ = (onset, freq);
    }
    fn note_off(&mut self, time: f64, peak_db: f32, freq: Option<f32>);
    /// The session ended (finished by itself or stopped by the user).
    fn stop(&mut self, io: &mut SessionIo) {
        io.silence();
    }
}
