//! The application shell: sidebar of exercise types, tuner strip, preset bar,
//! exercise area and the preferences window.

use std::collections::HashMap;
use std::path::PathBuf;

use bg_audio::{AnalysisConfig, AudioConfig, AudioEngine, SoundConfig};
use bg_core::ScoringThresholds;
use bg_core::preset::PresetStore;
use bg_ui::theme::ThemePrefs;
use bg_ui::widgets::button::{button, selectable};
use bg_ui::widgets::toolbar::show_toolbar;
use bg_ui::widgets::tuner::{TunerPrefs, tuner};
use bg_ui::{ExerciseCtx, PreferencePage, PrefsCtx, SettingsStore, egui, icons, theme};
use egui::RichText;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::menu::{AppMenu, MenuAction};
use crate::pages;
use crate::presets::{ExerciseSlot, PresetBar};

const PREFS_VIEWPORT: &str = "preferences";

/// Session state restored on the next launch (kept as JSON next to the settings).
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct AppState {
    selected: Option<String>,
    presets: HashMap<String, Uuid>,
    working: HashMap<String, Value>,
    /// Preferences window position and size (logical points).
    prefs_window: Option<WindowGeometry>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct WindowGeometry {
    pos: [f32; 2],
    size: [f32; 2],
}

pub struct BassGymApp {
    engine: AudioEngine,
    settings: SettingsStore,
    store: PresetStore,
    state_path: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    slots: Vec<ExerciseSlot>,
    selected: usize,
    preset_bar: PresetBar,
    prefs_open: bool,
    prefs_window: Option<WindowGeometry>,
    menu: Option<AppMenu>,
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
        let store = PresetStore::new(config_dir.as_ref().map_or_else(
            || std::env::temp_dir().join("bass-gym-presets"),
            |d| d.join("presets"),
        ));
        let state_path = config_dir.as_ref().map(|d| d.join("state.json"));

        crate::soundfonts::register();
        let engine = AudioEngine::new(
            settings.get::<AudioConfig>(pages::AUDIO),
            settings.get::<SoundConfig>(pages::SOUNDS),
            settings.get::<AnalysisConfig>(pages::ANALYSIS),
        );

        let state: AppState = state_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut slots: Vec<ExerciseSlot> = bg_exercises::all()
            .into_iter()
            .map(|et| ExerciseSlot::new(et, &store))
            .collect();
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
        let selected = std::env::var("BASS_GYM_EXERCISE")
            .ok()
            .or(state.selected)
            .and_then(|s| slots.iter().position(|x| x.et.id() == s))
            .unwrap_or(0);

        let mut app = Self {
            engine,
            settings,
            store,
            state_path,
            data_dir: dirs.as_ref().map(|d| d.data_dir().to_path_buf()),
            slots,
            selected,
            preset_bar: PresetBar::default(),
            prefs_open: false,
            prefs_window: state.prefs_window,
            menu: AppMenu::install(),
            pages,
            page: 0,
            scoring: ScoringThresholds::default(),
            tuner_prefs: TunerPrefs::default(),
            theme: None,
            ctx: cc.egui_ctx.clone(),
            settings_rev: u64::MAX,
            screenshot: std::env::var_os("BASS_GYM_SCREENSHOT")
                .map(|p| (PathBuf::from(p), std::time::Instant::now(), false)),
        };
        if std::env::var_os("BASS_GYM_FAKE_SESSION").is_some()
            && let Some(slot) = app.slots.get_mut(app.selected)
        {
            slot.et.debug_fake_session(&mut ExerciseCtx {
                engine: &mut app.engine,
                settings: &mut app.settings,
                scoring: &app.scoring,
                data_dir: app.data_dir.as_deref(),
            });
        }
        if let Some(page) = std::env::var("BASS_GYM_PREFS")
            .ok()
            .and_then(|id| app.pages.iter().position(|p| p.id() == id))
        {
            app.prefs_open = true;
            app.page = page;
            if app.screenshot.is_some() {
                // Screenshots only capture the main window: draw the preferences inside it.
                cc.egui_ctx.set_embed_viewports(true);
            }
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
            presets: self
                .slots
                .iter()
                .filter_map(|s| s.current.map(|c| (s.et.id().to_owned(), c)))
                .collect(),
            working: self
                .slots
                .iter()
                .map(|s| (s.et.id().to_owned(), s.et.save_preset()))
                .collect(),
            prefs_window: self.prefs_window,
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(&state)
            && let Err(e) = std::fs::write(path, text)
        {
            log::warn!("couldn't save app state: {e}");
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(icons::WAVE_SINE)
                    .size(22.0)
                    .color(theme::green()),
            );
            ui.label(
                RichText::new("BASS GYM")
                    .font(theme::mono(20.0))
                    .color(theme::text()),
            );
        });
        ui.add_space(14.0);
        theme::title_label(ui, "Exercises");
        ui.add_space(4.0);
        let running = self.slots.iter().any(|s| s.et.is_running());
        for (i, slot) in self.slots.iter().enumerate() {
            let selected = i == self.selected;
            let text = format!("{}  {}", slot.et.icon(), slot.et.name().to_uppercase());
            let resp = ui.add_enabled(
                !running || selected,
                selectable(selected, text).min_size(egui::vec2(ui.available_width(), 34.0)),
            );
            if resp.on_hover_text(slot.et.description()).clicked() {
                self.selected = i;
            }
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(10.0);
            let shortcut = if cfg!(target_os = "macos") {
                "⌘,"
            } else {
                "Ctrl+,"
            };
            if ui
                .add(
                    button(format!("{}  PREFERENCES", icons::GEAR))
                        .min_size(egui::vec2(ui.available_width(), 32.0)),
                )
                .on_hover_text(format!("Open the preferences ({shortcut})"))
                .clicked()
            {
                self.open_preferences(ui.ctx());
            }
            ui.add_space(4.0);
            let st = self.engine.status().clone();
            let dot = |ok: bool| if ok { theme::green() } else { theme::amber() };
            ui.label(
                RichText::new(format!("OUT {}", st.output.as_deref().unwrap_or("—")))
                    .small()
                    .color(dot(st.output.is_some())),
            );
            ui.label(
                RichText::new(format!("IN  {}", st.input.as_deref().unwrap_or("—")))
                    .small()
                    .color(dot(st.input.is_some())),
            );
            let xruns = self.engine.xruns();
            if xruns > 0 {
                ui.label(
                    RichText::new(format!("{} {xruns} XRUNS", icons::WARNING))
                        .small()
                        .color(theme::amber()),
                )
                .on_hover_text(
                    "Audio buffer under/overruns. Try a larger buffer size in Preferences › Audio.",
                );
            }
            ui.add_space(6.0);
            // Input monitoring: hear the bass through the output, with its round-trip delay.
            let mut sounds: SoundConfig = self.settings.get(pages::SOUNDS);
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut sounds.monitor_gain, 0.0..=2.0).show_value(false))
                    .on_hover_text("Monitor volume");
            });
            let toggle_color = |on: bool| if on { theme::green() } else { theme::cyan() };
            ui.horizontal(|ui| {
                let on = sounds.monitor;
                let label = format!("{}  MONITOR {}", icons::EAR, if on { "ON" } else { "OFF" });
                if ui
                    .add(selectable(on, label).color(toggle_color(on)))
                    .on_hover_text("Mix the bass input into the output")
                    .clicked()
                {
                    sounds.monitor = !on;
                }
                // Quiet practice: a low cut on the monitored bass (cutoff in Preferences › Sounds).
                let quiet = sounds.quiet;
                let tip = format!(
                    "Quiet practice {}: cut the monitored bass below {:.0} Hz, so it carries less through walls and floors (frequency in Preferences › Sounds)",
                    if quiet { "on" } else { "off" },
                    sounds.quiet_cutoff,
                );
                if ui
                    .add(selectable(quiet, icons::MOON).color(toggle_color(quiet)))
                    .on_hover_text(tip)
                    .clicked()
                {
                    sounds.quiet = !quiet;
                }
            });
            self.settings.set(pages::SOUNDS, &sounds);
            self.engine.set_sounds(sounds);
        });
    }

    /// Opens the preferences window, or brings it to the front.
    fn open_preferences(&mut self, ctx: &egui::Context) {
        if self.prefs_open {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of(PREFS_VIEWPORT),
                egui::ViewportCommand::Focus,
            );
        }
        self.prefs_open = true;
        ctx.request_repaint();
    }

    /// The preferences, in their own window.
    fn preferences(&mut self, ctx: &egui::Context) {
        let mut builder = egui::ViewportBuilder::default()
            .with_title("Preferences")
            .with_min_inner_size([560.0, 320.0]);
        builder = match self.prefs_window {
            Some(g) => builder.with_position(g.pos).with_inner_size(g.size),
            None => builder.with_inner_size([860.0, 620.0]),
        };
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of(PREFS_VIEWPORT),
            builder,
            |ui, _class| {
                let ctx = ui.ctx().clone();
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.prefs_open = false;
                }
                if let Some((outer, inner)) =
                    ctx.input(|i| i.viewport().outer_rect.zip(i.viewport().inner_rect))
                {
                    self.prefs_window = Some(WindowGeometry {
                        pos: outer.min.into(),
                        size: inner.size().into(),
                    });
                }
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(theme::panel())
                            .inner_margin(egui::Margin::same(14)),
                    )
                    .show(ui, |ui| {
                        let avail = ui.available_size();
                        ui.horizontal_top(|ui| {
                            // Page list.
                            ui.allocate_ui_with_layout(
                                egui::vec2(190.0, avail.y),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    for (i, p) in self.pages.iter().enumerate() {
                                        let text = format!("{}  {}", p.icon(), p.title());
                                        if ui
                                            .add(
                                                selectable(i == self.page, text)
                                                    .min_size(egui::vec2(186.0, 30.0)),
                                            )
                                            .clicked()
                                        {
                                            self.page = i;
                                        }
                                    }
                                },
                            );
                            ui.separator();
                            // The page: sections stacked vertically, scrolling when they overflow.
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), avail.y),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .id_salt(("prefs_page", self.page))
                                        .auto_shrink([false, false])
                                        .max_height(avail.y)
                                        .show(ui, |ui| {
                                            if let Some(page) = self.pages.get_mut(self.page) {
                                                let mut pctx = PrefsCtx {
                                                    engine: &mut self.engine,
                                                    settings: &mut self.settings,
                                                };
                                                page.ui(ui, &mut pctx);
                                            }
                                        });
                                },
                            );
                        });
                    });
            },
        );
    }
}

impl BassGymApp {
    fn take_screenshot(&mut self, ctx: &egui::Context) {
        let Some((path, started, requested)) = self.screenshot.as_mut() else {
            return;
        };
        ctx.request_repaint();
        if !*requested && started.elapsed().as_secs_f32() > 2.0 {
            *requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = image {
            let mut ppm = format!("P6 {} {} 255\n", image.size[0], image.size[1]).into_bytes();
            image
                .pixels
                .iter()
                .for_each(|p| ppm.extend_from_slice(&[p.r(), p.g(), p.b()]));
            match std::fs::write(&*path, ppm) {
                Ok(()) => log::info!("screenshot saved to {}", path.display()),
                Err(e) => log::warn!("couldn't save the screenshot: {e}"),
            }
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
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
            .frame(
                egui::Frame::new()
                    .fill(theme::bg())
                    .inner_margin(egui::Margin::symmetric(12, 4))
                    .stroke(egui::Stroke::new(1.0, theme::grid())),
            )
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ui, |ui| {
                tuner(
                    ui,
                    &self.engine.tuner(),
                    &self.engine.meter(),
                    &self.tuner_prefs,
                );
                ui.add_space(8.0);
                if let Some(slot) = self.slots.get_mut(self.selected) {
                    self.preset_bar.ui(ui, slot, &self.store);
                    ui.add_space(2.0);
                    let mut ectx = ExerciseCtx {
                        engine: &mut self.engine,
                        settings: &mut self.settings,
                        scoring: &self.scoring,
                        data_dir: self.data_dir.as_deref(),
                    };
                    // The Exercise Control Toolbar stays put; only the exercise area scrolls.
                    show_toolbar(ui, |tb| slot.et.toolbar(tb, &mut ectx));
                    slot.et.notices(ui, &mut ectx);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            slot.et.ui(ui, &mut ectx);
                        });
                }
            });

        // Preferences: app menu (macOS), Cmd/Ctrl + comma, or the sidebar button.
        let menu_action = self.menu.as_ref().and_then(AppMenu::poll);
        let shortcut = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::Comma,
            ))
        });
        if menu_action == Some(MenuAction::Preferences) || shortcut {
            self.open_preferences(&ctx);
        }
        if menu_action == Some(MenuAction::Quit) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.prefs_open {
            self.preferences(&ctx);
        }
        // Keyboard zoom (Cmd/Ctrl +/-/0) changes egui's zoom directly; keep the setting in sync.
        if let Some(t) = &self.theme {
            let z = ctx.zoom_factor();
            if (z - t.zoom).abs() > 1e-3 {
                let t = ThemePrefs {
                    zoom: z,
                    ..t.clone()
                };
                self.settings.set(ThemePrefs::ID, &t);
                self.theme = Some(t);
            }
        }
        self.refresh_settings();
        self.take_screenshot(&ctx);
    }

    /// Only the window geometry is persisted by eframe; egui's own memory isn't.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.engine.stop_session();
        self.settings.flush();
        self.save_state();
    }
}
