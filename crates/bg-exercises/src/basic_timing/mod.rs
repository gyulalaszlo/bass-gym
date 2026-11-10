//! Basic Timing: play the selected groove on time against the click.
//!
//! A tier-3 kit program (see `docs/architecture/ui-components.md`):
//! - `model.rs`: the model. `msg.rs`: what can happen to it.
//! - `sync.rs`: reads the outside world into messages, once a frame.
//! - `props.rs`: what each view region shows, derived once a frame.
//! - `update.rs`: how the model changes (pure, tested). `effects.rs`: engine calls.
//! - `view/`: one file per region. `theme.rs`: the root of the theme tree.
//!
//! This file only connects them to [`ExerciseType`].

mod effects;
mod model;
mod msg;
mod props;
mod sync;
mod theme;
mod update;
mod view;

use std::sync::Arc;

use anyhow::Result;
use bg_audio::AudioEngine;
use bg_core::preset::{PresetError, PresetSchema};
use bg_ui::kit::{self, Emit, ThemeCache};
use bg_ui::widgets::toolbar::Toolbar;
use bg_ui::widgets::tuner::TunerPrefs;
use bg_ui::{ExerciseCtx, ExerciseType, egui, icons};
use serde_json::Value;

pub use model::{BasicTimingData, ViewOptions};

use effects::Effect;
use model::Model;
use msg::Msg;
use theme::Theme;

use crate::fake_session;
use crate::timing_session::{TimingSession, lock};

/// The exercise's id. Never change it: presets and settings are stored under it.
const ID: &str = "basic_timing";

#[derive(Default)]
pub struct BasicTiming {
    model: Model,
    theme: ThemeCache<Theme>,
}

/// The program: [`update`](update::update) and [`effects::run`] for [`kit::dispatch`].
struct Screen;

impl kit::Program for Screen {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Ctx = AudioEngine;

    fn update(model: &mut Model, msg: Msg, fx: &mut Emit<Effect>) {
        update::update(model, msg, fx);
    }

    fn run(effect: Effect, engine: &mut AudioEngine, out: &mut Emit<Msg>) {
        effects::run(effect, engine, out);
    }
}

impl BasicTiming {
    /// Handles `msgs`, then repaints `ui` if there were any, since the views showed the
    /// state before them.
    fn dispatch(
        &mut self,
        engine: &mut AudioEngine,
        ui: Option<&egui::Context>,
        msgs: impl IntoIterator<Item = Msg>,
    ) {
        if kit::dispatch::<Screen>(&mut self.model, engine, msgs)
            && let Some(ui) = ui
        {
            ui.request_repaint();
        }
    }

    /// For messages that can't cause effects (no engine at hand).
    fn update_only(&mut self, msg: Msg) {
        let mut fx = Emit::new();
        update::update(&mut self.model, msg, &mut fx);
        debug_assert!(fx.is_empty(), "an effect with no engine to run it");
    }
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
        ID
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
        serde_json::to_value(&self.model.data).unwrap_or_default()
    }

    fn load_preset(&mut self, data: Value) -> Result<()> {
        let data: BasicTimingData = serde_json::from_value(data)?;
        self.update_only(Msg::Load(Box::new(data)));
        Ok(())
    }

    fn is_running(&self) -> bool {
        lock(&self.model.shared).running
    }

    /// The shell calls this first each frame, so it also runs `sync`.
    fn toolbar(&mut self, tb: &mut Toolbar, ctx: &mut ExerciseCtx) {
        let ui = tb.ui().ctx().clone();
        let msgs = sync::sync(&self.model, ctx);
        self.dispatch(ctx.engine, Some(&ui), msgs);
        let review = sync::review(&self.model, ctx);
        self.dispatch(ctx.engine, Some(&ui), review);

        let theme = self.theme.get();
        let mut out = Emit::new();
        view::toolbar::toolbar(&props::toolbar(&self.model), &theme, tb, &mut out);
        self.dispatch(ctx.engine, Some(&ui), out);
    }

    fn notices(&mut self, ui: &mut egui::Ui, _ctx: &mut ExerciseCtx) {
        let theme = self.theme.get();
        view::notices::notices(&props::notices(&self.model), &theme, ui);
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut ExerciseCtx) {
        let theme = self.theme.get();
        // The live part holds the timing lock while it shows; release it before the
        // settings.
        view::main::live(&props::main(&self.model), &theme, ui);
        let mut out = Emit::new();
        view::main::settings(&props::settings(&self.model), ui, &mut out);
        self.dispatch(ctx.engine, Some(ui.ctx()), out);

        let msgs = sync::review_sync(&self.model, ctx);
        self.dispatch(ctx.engine, Some(ui.ctx()), msgs);
        let mut out = Emit::new();
        let p = props::results(&self.model);
        view::results::results_region(&p, &theme, ctx.data_dir, ui.ctx(), &mut out);
        self.dispatch(ctx.engine, Some(ui.ctx()), out);
    }

    fn debug_fake_session(&mut self, ctx: &mut ExerciseCtx) {
        let cfg = fake_session::config(&self.model.data.session);
        let shared = &self.model.shared;
        let mut logic = TimingSession::new(cfg, ctx.scoring.clone(), shared.clone());
        let a4 = ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4;
        let recording = fake_session::run(&mut logic, shared, a4);
        self.dispatch(
            ctx.engine,
            None,
            [Msg::FakeSession {
                recording: Arc::new(recording),
                open_results: fake_session::opens_results(),
                open_stats: fake_session::opens_stats(),
            }],
        );
    }
}
