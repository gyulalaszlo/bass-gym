//! Basic Timing: play the selected groove on time against the click.

use anyhow::Result;
use std::sync::Arc;

use bg_core::SessionConfig;
use bg_core::preset::{PresetError, PresetSchema};
use bg_ui::recording::{RecordingPrefs, SessionCapture};
use bg_ui::view::ViewPrefs;
use bg_ui::widgets::beatgrid::Beatgrid;
use bg_ui::widgets::performance::performance_strip;
use bg_ui::widgets::results::{ResultsTab, results, results_dialog};
use bg_ui::widgets::session_panel::{session_panel, session_toolbar};
use bg_ui::widgets::session_review::{ReviewInput, SessionReview};
use bg_ui::widgets::toolbar::{Gap, Toolbar};
use bg_ui::widgets::tuner::TunerPrefs;
use bg_ui::{ExerciseCtx, ExerciseType, egui, icons, theme};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fake_session;
use crate::timing_session::{
    PERFORMANCE_HEIGHT, SharedTiming, TimingSession, last_time, lock, preview, timing_meter,
};

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
        Self {
            bars_behind: 1.0,
            bars_ahead: 2.0,
        }
    }
}

#[derive(Default)]
pub struct BasicTiming {
    data: BasicTimingData,
    shared: SharedTiming,
    /// A session ran since the last configuration change, so its results are shown.
    has_results: bool,
    /// The results dialog is open.
    show_results: bool,
    /// Running on the previous frame (to open the results when the session ends).
    was_running: bool,
    capture: SessionCapture,
    review: Option<SessionReview>,
    results_tab: ResultsTab,
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

    fn toolbar(&mut self, tb: &mut Toolbar, ctx: &mut ExerciseCtx) {
        let running = ctx.engine.session_running() && self.is_running();
        if self.was_running && !running && self.has_results {
            self.show_results = true;
        }
        self.was_running = running;
        self.capture.poll(ctx.engine);
        if !self.has_results {
            self.capture.clear();
            self.review = None;
        }
        self.update_review(ctx);
        if tb.transport(running) {
            if running {
                ctx.engine.stop_session();
            } else if ctx.engine.clock().is_running() {
                let logic = TimingSession::new(
                    self.data.session.clone(),
                    ctx.scoring.clone(),
                    self.shared.clone(),
                );
                let max_bytes = ctx
                    .settings
                    .get::<RecordingPrefs>(RecordingPrefs::ID)
                    .max_bytes();
                let id = ctx.engine.start_recorded(Box::new(logic), max_bytes);
                self.capture.begin(id);
                self.review = None;
                self.results_tab = ResultsTab::Stats;
                self.has_results = true;
                self.show_results = false;
            }
        }
        tb.gap(Gap::Small);
        if tb.results_button(self.has_results && !running) {
            self.show_results = true;
        }
        // Live status in the space between START and the settings.
        tb.gap_with(Gap::Large, |ui| status_line(ui, &self.shared, ctx, running));
        if tb.group(!running, |tb| session_toolbar(tb, &mut self.data.session)) {
            self.has_results = false;
        }
    }

    fn notices(&mut self, ui: &mut egui::Ui, ctx: &mut ExerciseCtx) {
        if !ctx.engine.clock().is_running() {
            ui.label(
                egui::RichText::new(format!(
                    "{} No audio output — check Preferences › Audio",
                    icons::WARNING
                ))
                .color(theme::amber()),
            );
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut ExerciseCtx) {
        let running = ctx.engine.session_running() && self.is_running();

        // Beat grid: the live / last session, or a preview of the configured groove.
        let steps = self.data.session.groove.steps_per_beat;
        let view = self.data.view.clone();
        let mut prefs: ViewPrefs = ctx.settings.get(ViewPrefs::ID);
        let zoom_before = prefs.grid_zoom;

        // Live performance, large, above the grid.
        let now = if running {
            ctx.engine.clock().now()
        } else {
            last_time(&self.shared)
        };
        let meter = timing_meter(
            &self.shared,
            self.has_results,
            now,
            prefs.indicator_bars,
            ctx.scoring,
        );
        performance_strip(ui, PERFORMANCE_HEIGHT, &[&meter]);
        ui.add_space(4.0);

        if self.has_results {
            let s = lock(&self.shared);
            Beatgrid {
                now,
                bars: &s.bars,
                matcher: &s.matcher,
                steps_per_beat: steps,
                bars_behind: view.bars_behind,
                bars_ahead: view.bars_ahead,
                chords: &[],
            }
            .show_zoomable(ui, 150.0, &mut prefs.grid_zoom);
        } else {
            let (bars, matcher) = preview(&self.data.session);
            let now = bars.first().map_or(0.0, |b| b.start);
            Beatgrid {
                now,
                bars: &bars,
                matcher: &matcher,
                steps_per_beat: steps,
                bars_behind: view.bars_behind,
                bars_ahead: view.bars_ahead,
                chords: &[],
            }
            .show_zoomable(ui, 150.0, &mut prefs.grid_zoom);
        }
        if prefs.grid_zoom != zoom_before {
            ctx.settings.set(ViewPrefs::ID, &prefs);
        }
        if running {
            ui.ctx().request_repaint();
        }
        ui.add_space(8.0);

        if session_panel(ui, &mut self.data.session, !running) {
            self.has_results = false;
        }

        self.show_results &= self.has_results;
        let stats = self
            .show_results
            .then(|| lock(&self.shared).matcher.stats());
        let scoring = ctx.scoring;
        let engine = &mut *ctx.engine;
        let review = &mut self.review;
        let tab = review.is_some().then_some(&mut self.results_tab);
        results_dialog(
            ui.ctx(),
            &mut self.show_results,
            "Results",
            tab,
            |ui, tab| match (tab, review.as_mut(), &stats) {
                (ResultsTab::Review, Some(r), _) => r.ui(ui, engine),
                (_, _, Some(stats)) => results(ui, stats, scoring, |_| {}),
                _ => {}
            },
        );
        if let Some(r) = self.review.as_mut() {
            r.tick(ctx.engine, self.show_results);
        }
    }

    fn debug_fake_session(&mut self, ctx: &mut ExerciseCtx) {
        let cfg = fake_session::config(&self.data.session);
        let mut logic = TimingSession::new(cfg, ctx.scoring.clone(), self.shared.clone());
        let a4 = ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4;
        let recording = fake_session::run(&mut logic, &self.shared, a4);
        self.capture.set(Arc::new(recording));
        self.has_results = true;
        self.show_results = true;
        self.results_tab = ResultsTab::Review;
    }
}

impl BasicTiming {
    /// Sets up the review once the session has fully ended.
    fn update_review(&mut self, ctx: &mut ExerciseCtx) {
        if self.review.is_some() || ctx.engine.session_running() {
            return;
        }
        let Some(recording) = self.capture.recording() else {
            return;
        };
        let shared = lock(&self.shared);
        self.review = Some(SessionReview::new(ReviewInput {
            bars: shared.bars.clone(),
            matcher: shared.matcher.clone(),
            chords: Vec::new(),
            scale: None,
            a4: ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4,
            recording: recording.clone(),
        }));
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
                    ui.label(mono(
                        format!("{} CLICK OFF", icons::SPEAKER_SLASH),
                        theme::amber(),
                    ));
                }
            }
            None => {
                ui.label(mono("READY".into(), theme::dim()));
            }
        }
    } else if ctx.engine.clock().is_running() {
        ui.label(mono("READY".into(), theme::dim()));
    }
}
