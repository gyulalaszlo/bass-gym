//! The application shell: sidebar of exercise types, tuner strip, preset bar,
//! exercise area and the preferences window.

use std::collections::HashMap;
use std::path::PathBuf;

use bg_audio::{AnalysisConfig, AudioConfig, AudioEngine, SoundConfig};
use bg_core::ScoringThresholds;
use bg_core::preset::PresetStore;
use bg_ui::theme::ThemePrefs;
use bg_ui::widgets::tuner::{TunerPrefs, tuner};
use bg_ui::{ExerciseCtx, PreferencePage, PrefsCtx, SettingsStore, egui, icons, theme};
use egui::RichText;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::pages;
use crate::presets::{ExerciseSlot, PresetBar};

/// Session state restored on the next launch (kept as JSON next to the settings).
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct AppState {
    selected: Option<String>,
    presets: HashMap<String, Uuid>,
    working: HashMap<String, Value>,
}

pub struct BassGymApp {
    engine: AudioEngine,
    settings: SettingsStore,
    store: PresetStore,
    state_path: Option<PathBuf>,
    slots: Vec<ExerciseSlot>,
    selected: usize,
    preset_bar: PresetBar,
    prefs_open: bool,
    pages: Vec<Box<dyn PreferencePage>>,
    page: usize,
    scoring: ScoringThresholds,
    tuner_prefs: TunerPrefs,
    theme: Option<ThemePrefs>,
    ctx: egui::Context,
    settings_rev: u64,
    /// Debugging aid: `BASS_GYM_SCREENSHOT=<file.ppm>` saves the window after a few
    /// frames and quits (`BASS_GYM_EXERCISE=<id>` picks the exercise shown,
    /// `BASS_GYM_PREFS=<page id>` opens the preferences).
    screenshot: Option<(PathBuf, std::time::Instant, bool)>,
}

impl BassGymApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let dirs = directories::ProjectDirs::from("", "", "bass-gym");
        let config_dir = dirs.as_ref().map(|d| d.config_dir().to_path_buf());
        if let Some(d) = &config_dir {
            log::info!("settings and presets in {}", d.display());
        }
        let settings = SettingsStore::load(config_dir.as_ref().map(|d| d.join("settings.toml")));
        let store = PresetStore::new(config_dir.as_ref().map_or_else(|| std::env::temp_dir().join("bass-gym-presets"), |d| d.join("presets")));
        let state_path = config_dir.as_ref().map(|d| d.join("state.json"));

        crate::soundfonts::register();
        let engine = AudioEngine::new(settings.get::<AudioConfig>(pages::AUDIO), settings.get::<SoundConfig>(pages::SOUNDS), settings.get::<AnalysisConfig>(pages::ANALYSIS));

        let state: AppState = state_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut slots: Vec<ExerciseSlot> = bg_exercises::all().into_iter().map(|et| ExerciseSlot::new(et, &store)).collect();
        let mut pages = pages::builtin();
        for slot in &mut slots {
            let id = slot.et.id();
            match state.presets.get(id) {
                Some(&p) if slot.load(p).is_ok() => {}
                _ => slot.load_default(),
            }
            if let Some(w) = state.working.get(id) {
                slot.restore_working(w.clone());
            }
            pages.extend(slot.et.preference_pages());
        }
        let selected = std::env::var("BASS_GYM_EXERCISE").ok().or(state.selected).and_then(|s| slots.iter().position(|x| x.et.id() == s)).unwrap_or(0);

        let mut app = Self {
            engine,
            settings,
            store,
            state_path,
            slots,
            selected,
            preset_bar: PresetBar::default(),
            prefs_open: false,
            pages,
            page: 0,
            scoring: ScoringThresholds::default(),
            tuner_prefs: TunerPrefs::default(),
            theme: None,
            ctx: cc.egui_ctx.clone(),
            settings_rev: u64::MAX,
            screenshot: std::env::var_os("BASS_GYM_SCREENSHOT").map(|p| (PathBuf::from(p), std::time::Instant::now(), false)),
        };
        if let Some(page) = std::env::var("BASS_GYM_PREFS").ok().and_then(|id| app.pages.iter().position(|p| p.id() == id)) {
            app.prefs_open = true;
            app.page = page;
        }
        app.refresh_settings();
        app
    }

    fn refresh_settings(&mut self) {
        if self.settings.revision() != self.settings_rev {
            self.settings_rev = self.settings.revision();
            self.scoring = self.settings.get(pages::SCORING);
            self.tuner_prefs = self.settings.get(TunerPrefs::ID);
            let t: ThemePrefs = self.settings.get(ThemePrefs::ID);
            if self.theme.as_ref() != Some(&t) {
                theme::apply(&self.ctx, &t);
                self.ctx.set_zoom_factor(t.zoom.clamp(0.5, 2.5));
                self.theme = Some(t);
            }
            self.settings.flush();
        }
    }

    fn save_state(&self) {
        let Some(path) = &self.state_path else { return };
        let state = AppState {
            selected: self.slots.get(self.selected).map(|s| s.et.id().to_owned()),
            presets: self.slots.iter().filter_map(|s| s.current.map(|c| (s.et.id().to_owned(), c))).collect(),
            working: self.slots.iter().map(|s| (s.et.id().to_owned(), s.et.save_preset())).collect(),
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(&state)
            && let Err(e) = std::fs::write(path, text) {
                log::warn!("couldn't save app state: {e}");
            }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::WAVE_SINE).size(22.0).color(theme::green()));
            ui.label(RichText::new("BASS GYM").font(theme::mono(20.0)).color(theme::text()));
        });
        ui.add_space(14.0);
        theme::title_label(ui, "Exercises");
        ui.add_space(4.0);
        let running = self.slots.iter().any(|s| s.et.is_running());
        for (i, slot) in self.slots.iter().enumerate() {
            let selected = i == self.selected;
            let color = if selected { theme::text() } else { theme::cyan() };
            let text = RichText::new(format!("{}  {}", slot.et.icon(), slot.et.name().to_uppercase())).color(color);
            let resp = ui.add_enabled(!running || selected, egui::Button::selectable(selected, text).min_size(egui::vec2(ui.available_width(), 34.0)));
            if resp.on_hover_text(slot.et.description()).clicked() {
                self.selected = i;
            }
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(10.0);
            if ui.add(egui::Button::new(format!("{}  PREFERENCES", icons::GEAR)).min_size(egui::vec2(ui.available_width(), 32.0))).clicked() {
                self.prefs_open = true;
            }
            ui.add_space(4.0);
            let st = self.engine.status().clone();
            let dot = |ok: bool| if ok { theme::green() } else { theme::amber() };
            ui.label(RichText::new(format!("OUT {}", st.output.as_deref().unwrap_or("—"))).small().color(dot(st.output.is_some())));
            ui.label(RichText::new(format!("IN  {}", st.input.as_deref().unwrap_or("—"))).small().color(dot(st.input.is_some())));
            let xruns = self.engine.xruns();
            if xruns > 0 {
                ui.label(RichText::new(format!("{} {xruns} XRUNS", icons::WARNING)).small().color(theme::amber()))
                    .on_hover_text("Audio buffer under/overruns. Try a larger buffer size in Preferences › Audio.");
            }
            ui.add_space(6.0);
            // Input monitoring: hear the bass through the output, with its round-trip delay.
            let mut sounds: SoundConfig = self.settings.get(pages::SOUNDS);
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut sounds.monitor_gain, 0.0..=2.0).show_value(false)).on_hover_text("Monitor volume");
            });
            let on = sounds.monitor;
            let label = RichText::new(format!("{}  MONITOR {}", icons::EAR, if on { "ON" } else { "OFF" })).color(if on { theme::green() } else { theme::cyan() });
            ui.toggle_value(&mut sounds.monitor, label).on_hover_text("Mix the bass input into the output");
            self.settings.set(pages::SOUNDS, &sounds);
            self.engine.set_sounds(sounds);
        });
    }

    fn preferences(&mut self, ctx: &egui::Context) {
        let mut open = self.prefs_open;
        egui::Window::new(RichText::new(format!("{}  Preferences", icons::GEAR)))
            .open(&mut open)
            .default_size([860.0, 620.0])
            .min_size([560.0, 320.0])
            .resizable(true)
            .vscroll(false)
            .collapsible(false)
            .show(ctx, |ui| {
                let avail = ui.available_size();
                ui.horizontal_top(|ui| {
                    // Page list.
                    ui.allocate_ui_with_layout(egui::vec2(190.0, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                        for (i, p) in self.pages.iter().enumerate() {
                            let text = RichText::new(format!("{}  {}", p.icon(), p.title())).color(if i == self.page { theme::text() } else { theme::cyan() });
                            if ui.add(egui::Button::selectable(i == self.page, text).min_size(egui::vec2(186.0, 30.0))).clicked() {
                                self.page = i;
                            }
                        }
                    });
                    ui.separator();
                    // The page: sections stacked vertically, scrolling when they overflow.
                    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical().id_salt(("prefs_page", self.page)).auto_shrink([false, false]).max_height(avail.y).show(ui, |ui| {
                            if let Some(page) = self.pages.get_mut(self.page) {
                                let mut pctx = PrefsCtx { engine: &mut self.engine, settings: &mut self.settings };
                                page.ui(ui, &mut pctx);
                            }
                        });
                    });
                });
            });
        self.prefs_open = open;
    }
}

impl BassGymApp {
    fn take_screenshot(&mut self, ctx: &egui::Context) {
        let Some((path, started, requested)) = self.screenshot.as_mut() else { return };
        ctx.request_repaint();
        if !*requested && started.elapsed().as_secs_f32() > 2.0 {
            *requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let Some(image) = image {
            let mut ppm = format!("P6 {} {} 255\n", image.size[0], image.size[1]).into_bytes();
            image.pixels.iter().for_each(|p| ppm.extend_from_slice(&[p.r(), p.g(), p.b()]));
            match std::fs::write(&*path, ppm) {
                Ok(()) => log::info!("screenshot saved to {}", path.display()),
                Err(e) => log::warn!("couldn't save the screenshot: {e}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for BassGymApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.engine.poll();
        let ctx = ui.ctx().clone();

        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(240.0)
            .frame(egui::Frame::new().fill(theme::bg()).inner_margin(egui::Margin::symmetric(12, 4)).stroke(egui::Stroke::new(1.0, theme::grid())))
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::panel()).inner_margin(egui::Margin::same(14))).show(ui, |ui| {
            tuner(ui, &self.engine.tuner(), &self.engine.meter(), &self.tuner_prefs);
            ui.add_space(8.0);
            if let Some(slot) = self.slots.get_mut(self.selected) {
                self.preset_bar.ui(ui, slot, &self.store);
                ui.add_space(4.0);
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut ectx = ExerciseCtx { engine: &mut self.engine, settings: &mut self.settings, scoring: &self.scoring };
                    slot.et.ui(ui, &mut ectx);
                });
            }
        });

        if self.prefs_open {
            self.preferences(&ctx);
        }
        // Keyboard zoom (Cmd/Ctrl +/-/0) changes egui's zoom directly; keep the setting in sync.
        if let Some(t) = &self.theme {
            let z = ctx.zoom_factor();
            if (z - t.zoom).abs() > 1e-3 {
                let t = ThemePrefs { zoom: z, ..t.clone() };
                self.settings.set(ThemePrefs::ID, &t);
                self.theme = Some(t);
            }
        }
        self.refresh_settings();
        self.take_screenshot(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.engine.stop_session();
        self.settings.flush();
        self.save_state();
    }
}
