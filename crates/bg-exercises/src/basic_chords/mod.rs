//! Basic chords: a simulated keyboard player comps a progression in the chosen key and
//! the bassist plays chord-compatible notes on the groove. A piano keyboard shows what
//! the player's hands are doing.
//!
//! A tier-3 kit program (see `docs/architecture/ui-components.md`), laid out like
//! Basic timing:
//! - `data.rs`: the preset data and pure functions on it. `accompaniment.rs`: the
//!   piano player. `prefs.rs`: its preference page.
//! - `model.rs`, `msg.rs`, `sync.rs`, `props.rs`, `update.rs` (pure, tested),
//!   `effects.rs` (engine calls), `theme.rs`.
//! - `view/`: one file per region, plus the views they use; `view/config/` holds the
//!   settings sections.
//!
//! This file only connects them to [`ExerciseType`], and keeps the preset migrations.

pub mod accompaniment;
mod data;
mod effects;
mod model;
mod msg;
pub mod prefs;
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
use bg_ui::{ExerciseCtx, ExerciseType, PreferencePage, egui, icons};
use serde_json::{Value, json};

pub use data::{
    BasicChordsData, ChordLength, FixedSource, KeyboardOptions, ProgressionKind,
    ProgressionSettings, ScoringOptions, resolve_progression,
};

use effects::Effect;
use model::Model;
use msg::Msg;
use prefs::BasicChordsPage;
use theme::Theme;

use crate::fake_session;
use crate::timing_session::lock;

/// The exercise's id, kept from the "Chord interaction" preview so its presets still
/// apply. Never change it: presets and settings are stored under it.
const ID: &str = "diatonic_chords";

#[derive(Default)]
pub struct BasicChords {
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

impl BasicChords {
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

impl PresetSchema for BasicChords {
    fn current_version(&self) -> u32 {
        3
    }

    fn migrate_step(&self, from: u32, mut data: Value) -> Result<Value, PresetError> {
        match from {
            // v1 ("Chord interaction" preview): sevenths flag, random progression length
            // and whole bars per chord.
            1 => {
                let o = data
                    .as_object_mut()
                    .ok_or_else(|| PresetError::Invalid("expected an object".into()))?;
                let sevenths = o
                    .remove("sevenths")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let len = o
                    .remove("progression_len")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(4);
                let bars = o
                    .remove("bars_per_chord")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1);
                o.insert("jazziness".into(), json!(if sevenths { 0.35 } else { 0.1 }));
                o.insert(
                    "progression".into(),
                    json!({ "kind": "Random", "random_len": len }),
                );
                o.insert(
                    "chord_length".into(),
                    json!(match bars {
                        0 | 1 => "One",
                        2 => "Two",
                        _ => "Four",
                    }),
                );
                Ok(data)
            }
            // v2: the key moved into the session settings, shared by every exercise.
            2 => {
                let o = data
                    .as_object_mut()
                    .ok_or_else(|| PresetError::Invalid("expected an object".into()))?;
                if let Some(scale) = o.remove("scale") {
                    let session = o.entry("session").or_insert_with(|| json!({}));
                    if let Some(session) = session.as_object_mut() {
                        session.insert("key".into(), scale);
                    }
                }
                Ok(data)
            }
            _ => Err(PresetError::NoMigration(from)),
        }
    }
}

impl ExerciseType for BasicChords {
    fn id(&self) -> &'static str {
        ID
    }

    fn name(&self) -> &'static str {
        "Basic chords"
    }

    fn icon(&self) -> &'static str {
        icons::PIANO_KEYS
    }

    fn description(&self) -> &'static str {
        "A keyboard player comps a progression; play chord-compatible notes on the groove."
    }

    fn default_preset(&self) -> Value {
        serde_json::to_value(BasicChordsData::default()).unwrap_or_default()
    }

    fn save_preset(&self) -> Value {
        serde_json::to_value(&self.model.data).unwrap_or_default()
    }

    fn load_preset(&mut self, data: Value) -> Result<()> {
        let data: BasicChordsData = serde_json::from_value(data)?;
        self.update_only(Msg::Load(Box::new(data)));
        Ok(())
    }

    fn is_running(&self) -> bool {
        lock(&self.model.timing).running
    }

    fn preference_pages(&self) -> Vec<Box<dyn PreferencePage>> {
        vec![Box::new(BasicChordsPage::default())]
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
        view::live::live(&props::live(&self.model), &theme, ui);

        let mut out = Emit::new();
        view::config::config(&props::config(&self.model), &theme, ui, &mut out);
        self.dispatch(ctx.engine, Some(ui.ctx()), out);

        let msgs = sync::review_sync(&self.model, ctx);
        self.dispatch(ctx.engine, Some(ui.ctx()), msgs);
        let mut out = Emit::new();
        let p = props::results(&self.model);
        view::results::results_region(&p, &theme, ctx.data_dir, ui.ctx(), &mut out);
        self.dispatch(ctx.engine, Some(ui.ctx()), out);
    }

    fn debug_fake_session(&mut self, ctx: &mut ExerciseCtx) {
        let msgs = sync::sync(&self.model, ctx);
        self.dispatch(ctx.engine, None, msgs);
        let cfg = fake_session::config(&self.model.data.session);
        self.update_only(Msg::Session(Box::new(cfg)));
        let start = match update::start(&self.model) {
            Ok(s) => s,
            Err(e) => {
                self.model.error = Some(format!("Can't start: {e}"));
                return;
            }
        };
        let mut logic = effects::session(&start);
        let recording = fake_session::run(&mut logic, &self.model.timing, start.a4);
        self.dispatch(
            ctx.engine,
            None,
            [Msg::FakeSession {
                recording: Arc::new(recording),
                open_results: fake_session::opens_results(),
                seed: start.seed,
            }],
        );
    }
}
