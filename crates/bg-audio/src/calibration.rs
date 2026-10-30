//! Latency calibration: plays a steady click and measures how late the detected notes
//! arrive. Works with a loopback cable (output → input) or by the player plucking
//! along with the click.

use std::sync::{Arc, Mutex};

use bg_core::plan::ClickKind;
use bg_sound::{SoundEvent, SoundKind};

use crate::session::{SessionIo, SessionLogic, SessionState, Slot};

pub const CALIBRATION_CLICKS: usize = 16;
const INTERVAL: f64 = 0.5;
const LEAD_IN: usize = 4;

#[derive(Clone, Debug, Default)]
pub struct CalibrationState {
    pub running: bool,
    /// Raw offsets (detected − scheduled) in ms, for the clicks that were matched.
    pub offsets_ms: Vec<f32>,
    pub clicks_done: usize,
    /// Median offset, available when enough clicks matched.
    pub result_ms: Option<f32>,
}

pub struct Calibration {
    state: Arc<Mutex<CalibrationState>>,
    clicks: Vec<f64>,
    latency: f64,
}

impl Calibration {
    pub fn new(state: Arc<Mutex<CalibrationState>>) -> Self {
        Self { state, clicks: Vec::new(), latency: 0.0 }
    }

    fn with_state(&self, f: impl FnOnce(&mut CalibrationState)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }
}

impl SessionLogic for Calibration {
    fn start(&mut self, io: &mut SessionIo) {
        self.latency = io.latency;
        let start = io.now + 0.4;
        self.clicks = (0..LEAD_IN + CALIBRATION_CLICKS).map(|i| start + i as f64 * INTERVAL).collect();
        for (i, &t) in self.clicks.iter().enumerate() {
            let kind = if i < LEAD_IN { ClickKind::Beat } else { ClickKind::Accent };
            io.play(t, Slot::Click, SoundEvent { kind: SoundKind::Click(kind), gain: 1.0 });
        }
        self.with_state(|s| *s = CalibrationState { running: true, ..Default::default() });
    }

    fn tick(&mut self, io: &mut SessionIo) -> SessionState {
        let done = self.clicks.iter().skip(LEAD_IN).filter(|&&t| t < io.now).count();
        self.with_state(|s| s.clicks_done = done);
        if self.clicks.last().is_some_and(|&t| io.now > t + 0.4) { SessionState::Finished } else { SessionState::Running }
    }

    fn note_on(&mut self, time: f64, _level_db: f32) {
        let raw = time + self.latency;
        // Match to the nearest measured click, within ±200 ms.
        let Some(&click) = self.clicks[LEAD_IN..].iter().min_by(|a, b| (raw - **a).abs().total_cmp(&(raw - **b).abs())) else { return };
        if (raw - click).abs() < 0.2 {
            self.with_state(|s| s.offsets_ms.push(((raw - click) * 1000.0) as f32));
        }
    }

    fn note_off(&mut self, _: f64, _: f32, _: Option<f32>) {}

    fn stop(&mut self, io: &mut SessionIo) {
        io.silence();
        self.with_state(|s| {
            s.running = false;
            if s.offsets_ms.len() >= CALIBRATION_CLICKS / 2 {
                let mut v = s.offsets_ms.clone();
                v.sort_by(f32::total_cmp);
                s.result_ms = Some(v[v.len() / 2]);
            }
        });
    }
}
