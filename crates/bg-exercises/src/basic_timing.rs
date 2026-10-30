//! Basic Timing: play the selected groove on time against the click.

use anyhow::Result;
use bg_core::preset::{PresetError, PresetSchema};
use bg_core::SessionConfig;
use bg_ui::widgets::beatgrid::Beatgrid;
use bg_ui::widgets::results::results;
use bg_ui::widgets::session_panel::session_panel;
use bg_ui::widgets::transport_button;
use bg_ui::{ExerciseCtx, ExerciseType, egui, icons, theme};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::timing_session::{SharedTiming, TimingSession, last_time, lock, preview};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct BasicTimingData {
    pub session: SessionConfig,
    pub view: ViewOptions,
}


#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewOptions {
    pub bars_behind: f64,
    pub bars_ahead: f64,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self { bars_behind: 1.0, bars_ahead: 2.0 }
    }
}

#[derive(Default)]
pub struct BasicTiming {
    data: BasicTimingData,
    shared: SharedTiming,
    /// A session ran since the last configuration change, so its results are shown.
    has_results: bool,
}

impl PresetSchema for BasicTiming {
    fn current_version(&self) -> u32 {
        1
    }

    fn migrate_step(&self, from: u32, _data: Value) -> Result<Value, PresetError> {
        // v1 is the first schema; future versions add steps here (from = 1 → 2, ...).
        Err(PresetError::NoMigration(from))
    }
}

impl ExerciseType for BasicTiming {
    fn id(&self) -> &'static str {
        "basic_timing"
    }

    fn name(&self) -> &'static str {
        "Basic timing"
    }

    fn icon(&self) -> &'static str {
        icons::METRONOME
    }

    fn description(&self) -> &'static str {
        "Play the groove in time with the click. Timing, length and level of every note are scored."
    }

    fn default_preset(&self) -> Value {
        serde_json::to_value(BasicTimingData::default()).unwrap_or_default()
    }

    fn save_preset(&self) -> Value {
        serde_json::to_value(&self.data).unwrap_or_default()
    }

    fn load_preset(&mut self, data: Value) -> Result<()> {
        self.data = serde_json::from_value(data)?;
        self.has_results = false;
        Ok(())
    }

    fn is_running(&self) -> bool {
        lock(&self.shared).running
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut ExerciseCtx) {
        let running = ctx.engine.session_running() && self.is_running();
        let space = ui.input(|i| i.key_pressed(egui::Key::Space)) && !ui.ctx().egui_wants_keyboard_input();

        ui.horizontal(|ui| {
            if transport_button(ui, running) || space {
                if running {
                    ctx.engine.stop_session();
                } else if ctx.engine.clock().is_running() {
                    let logic = TimingSession::new(self.data.session.clone(), ctx.scoring.clone(), self.shared.clone());
                    ctx.engine.start_session(Box::new(logic));
                    self.has_results = true;
                }
            }
            ui.add_space(8.0);
            status_line(ui, &self.shared, ctx, running);
        });
        if !ctx.engine.clock().is_running() {
            ui.label(egui::RichText::new(format!("{} No audio output — check Preferences › Audio", icons::WARNING)).color(theme::amber()));
        }
        ui.add_space(6.0);

        // Beat grid: the live / last session, or a preview of the configured groove.
        let steps = self.data.session.groove.steps_per_beat;
        let view = self.data.view.clone();
        if self.has_results {
            let now = if running { ctx.engine.clock().now() } else { last_time(&self.shared) };
            let s = lock(&self.shared);
            Beatgrid { now, bars: &s.bars, matcher: &s.matcher, steps_per_beat: steps, bars_behind: view.bars_behind, bars_ahead: view.bars_ahead, chords: &[] }.show(ui, 150.0);
        } else {
            let (bars, matcher) = preview(&self.data.session);
            let now = bars.first().map_or(0.0, |b| b.start);
            Beatgrid { now, bars: &bars, matcher: &matcher, steps_per_beat: steps, bars_behind: view.bars_behind, bars_ahead: view.bars_ahead, chords: &[] }.show(ui, 150.0);
        }
        if running {
            ui.ctx().request_repaint();
        }
        ui.add_space(8.0);

        if self.has_results {
            let stats = lock(&self.shared).matcher.stats();
            theme::section(ui, if running { "Live results" } else { "Results" }, |ui| results(ui, &stats, ctx.scoring));
            ui.add_space(8.0);
        }

        if session_panel(ui, &mut self.data.session, !running) {
            self.has_results = false;
        }
    }
}

fn status_line(ui: &mut egui::Ui, shared: &SharedTiming, ctx: &ExerciseCtx, running: bool) {
    let s = lock(shared);
    let now = ctx.engine.clock().now();
    let bar = s.bars.iter().rev().find(|b| b.start <= now);
    let mono = |t: String, c| egui::RichText::new(t).font(theme::mono(16.0)).color(c);
    if running {
        match bar {
            Some(b) if b.is_count_in() => {
                ui.label(mono("COUNT-IN".into(), theme::amber()));
            }
            Some(b) => {
                ui.label(mono(format!("BAR {:>3}", b.index + 1), theme::text()));
                ui.label(mono(format!("{:>5.1} BPM", b.bpm), theme::green()));
                if b.muted {
                    ui.label(mono(format!("{} CLICK OFF", icons::SPEAKER_SLASH), theme::amber()));
                }
            }
            None => {
                ui.label(mono("READY".into(), theme::dim()));
            }
        }
    } else {
        ui.label(egui::RichText::new("Space: start / stop").small().color(theme::dim()));
    }
}
