//! The application shell: sidebar of exercise types, tuner strip, preset bar, exercise
//! area and the preferences window.
//!
//! The root of the kit's tree. Its views (the sidebar, the preset bar, the page list)
//! send [`Msg`]s, and [`BassGymApp::update`] carries them out: the root owns the engine
//! and the stores, so its effects run there directly. Exercises are programs of their
//! own, and preference pages run their own glue.
//!
//! - `sidebar.rs`, `prefs.rs` (the window's page list and columns): views.
//! - `state.rs`: what survives a restart. `screenshot.rs`: the debugging aid.
//! - `theme.rs`: the shell's theme root.

mod prefs;
mod screenshot;
mod sidebar;
mod state;
mod theme;

use std::path::PathBuf;

use bg_audio::{AnalysisConfig, AudioConfig, AudioEngine, SoundConfig};
use bg_core::ScoringThresholds;
use bg_core::preset::PresetStore;
use bg_ui::kit::{self, Emit, ThemeCache};
use bg_ui::theme::ThemePrefs;
use bg_ui::views::tuner::{self, Tuner, TunerPrefs};
use bg_ui::widgets::toolbar::show_toolbar;
use bg_ui::{ExerciseCtx, PreferencePage, PrefsCtx, SettingsStore, egui};

use crate::menu::{AppMenu, MenuAction};
use crate::pages;
use crate::presets::{self, ExerciseSlot, PresetBar, bar};
use screenshot::Screenshot;
use sidebar::Sidebar;
use state::{AppState, WindowGeometry};
use theme::Theme;

const PREFS_VIEWPORT: &str = "preferences";

pub struct BassGymApp {
    engine: AudioEngine,
    settings: SettingsStore,
    store: PresetStore,
    state_path: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    slots: Vec<ExerciseSlot>,
    selected: usize,
    preset_bar: bar::State,
    /// How the last preset operation went (only errors are shown).
    preset_message: Option<(String, bool)>,
    prefs_open: bool,
    prefs_window: Option<WindowGeometry>,
    menu: Option<AppMenu>,
    pages: Vec<Box<dyn PreferencePage>>,
    page: usize,
    scoring: ScoringThresholds,
    tuner_prefs: TunerPrefs,
    theme: Option<ThemePrefs>,
    app_theme: ThemeCache<Theme>,
    ctx: egui::Context,
    settings_rev: u64,
    screenshot: Option<Screenshot>,
}

/// What the shell's views ask for.
enum Msg {
    Select(usize),
    OpenPreferences,
    /// Monitoring changed in the sidebar.
    Sounds(SoundConfig),
    Preset(bar::Emission),
    Page(usize),
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

        let state = AppState::load(state_path.as_deref());
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
            preset_bar: bar::State::default(),
            preset_message: None,
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
            screenshot: Screenshot::from_env(),
            app_theme: ThemeCache::default(),
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
                bg_ui::theme::apply(&self.ctx, &t);
                self.ctx.set_zoom_factor(t.zoom.clamp(0.5, 2.5));
                self.theme = Some(t);
            }
            self.settings.flush();
        }
    }

    fn save_state(&self) {
        let Some(path) = &self.state_path else { return };
        AppState {
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
        }
        .save(path);
    }

    /// Carries out what the views asked.
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Select(i) => self.selected = i,
            Msg::OpenPreferences => self.open_preferences(&self.ctx.clone()),
            Msg::Sounds(s) => {
                self.settings.set(pages::SOUNDS, &s);
                self.engine.set_sounds(s);
            }
            Msg::Preset(e) => {
                if let Some(slot) = self.slots.get_mut(self.selected) {
                    self.preset_message =
                        presets::run(slot, &self.store, e).err().map(|e| (e, false));
                }
            }
            Msg::Page(i) => self.page = i,
        }
    }

    fn dispatch(&mut self, msgs: impl IntoIterator<Item = Msg>) {
        let mut any = false;
        for m in msgs {
            self.update(m);
            any = true;
        }
        if any {
            self.ctx.request_repaint();
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, theme: &Theme) -> Vec<Msg> {
        let exercises: Vec<sidebar::Item> = self
            .slots
            .iter()
            .map(|s| sidebar::Item {
                icon: s.et.icon(),
                name: s.et.name(),
                description: s.et.description(),
            })
            .collect();
        let sounds: SoundConfig = self.settings.get(pages::SOUNDS);
        let props = sidebar::Props {
            exercises: &exercises,
            selected: self.selected,
            running: self.slots.iter().any(|s| s.et.is_running()),
            status: self.engine.status(),
            xruns: self.engine.xruns(),
            sounds: &sounds,
        };
        kit::show::<Sidebar>(ui, "sidebar", &props, &theme.sidebar)
            .into_iter()
            .map(|e| match e {
                sidebar::Emission::Select(i) => Msg::Select(i),
                sidebar::Emission::OpenPreferences => Msg::OpenPreferences,
                sidebar::Emission::Sounds(s) => Msg::Sounds(s),
            })
            .collect()
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
    fn preferences(&mut self, ctx: &egui::Context, theme: &Theme) {
        let t = &theme.prefs;
        let mut builder = egui::ViewportBuilder::default()
            .with_title("Preferences")
            .with_min_inner_size(t.min_size);
        builder = match self.prefs_window {
            Some(g) => builder.with_position(g.pos).with_inner_size(g.size),
            None => builder.with_inner_size(t.size),
        };
        let mut msgs = Emit::new();
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
                let titles: Vec<_> = self.pages.iter().map(|p| (p.icon(), p.title())).collect();
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(t.fill)
                            .inner_margin(egui::Margin::same(t.margin)),
                    )
                    .show(ui, |ui| {
                        let list = prefs::Props {
                            pages: &titles,
                            selected: self.page,
                        };
                        prefs::columns(
                            ui,
                            t,
                            self.page,
                            |ui| {
                                let picked = kit::show::<prefs::PageList>(ui, "pages", &list, t);
                                msgs.extend_map(picked, Msg::Page);
                            },
                            |ui| {
                                // A page runs its own glue against the engine and settings.
                                if let Some(page) = self.pages.get_mut(self.page) {
                                    let mut pctx = PrefsCtx {
                                        engine: &mut self.engine,
                                        settings: &mut self.settings,
                                    };
                                    page.ui(ui, &mut pctx);
                                }
                            },
                        );
                    });
            },
        );
        self.dispatch(msgs);
    }
}

impl eframe::App for BassGymApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.engine.poll();
        let ctx = ui.ctx().clone();
        let theme = self.app_theme.get();

        let sidebar_msgs = egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(theme.sidebar_width)
            .frame(
                egui::Frame::new()
                    .fill(theme.sidebar_fill)
                    .inner_margin(egui::Margin::symmetric(
                        theme.sidebar_margin[0],
                        theme.sidebar_margin[1],
                    ))
                    .stroke(theme.sidebar_frame),
            )
            .show(ui, |ui| self.sidebar(ui, &theme))
            .inner;
        self.dispatch(sidebar_msgs);

        let mut preset_msgs = Vec::new();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme.panel_fill)
                    .inner_margin(egui::Margin::same(theme.panel_margin)),
            )
            .show(ui, |ui| {
                let (state, meter) = (self.engine.tuner(), self.engine.meter());
                let props = tuner::Props {
                    state: &state,
                    meter: &meter,
                    prefs: &self.tuner_prefs,
                };
                kit::show::<Tuner>(ui, "tuner", &props, &theme.tuner);
                ui.add_space(theme.tuner_gap);
                if let Some(slot) = self.slots.get_mut(self.selected) {
                    let props = bar::Props {
                        name: slot.current_name(),
                        current: slot.current,
                        presets: &slot.presets,
                        dirty: slot.is_dirty(),
                        locked: slot.et.is_running(),
                        message: self.preset_message.as_ref(),
                    };
                    preset_msgs = kit::mount::<PresetBar>(
                        ui,
                        "presets",
                        &mut self.preset_bar,
                        &props,
                        &theme.presets,
                    );
                    ui.add_space(theme.presets_gap);
                    let mut ectx = ExerciseCtx {
                        engine: &mut self.engine,
                        settings: &mut self.settings,
                        scoring: &self.scoring,
                        data_dir: self.data_dir.as_deref(),
                    };
                    // The Exercise Control Toolbar stays put; only the exercise area scrolls.
                    show_toolbar(ui, &theme.toolbar, |tb| slot.et.toolbar(tb, &mut ectx));
                    slot.et.notices(ui, &mut ectx);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            slot.et.ui(ui, &mut ectx);
                        });
                }
            });
        self.dispatch(preset_msgs.into_iter().map(Msg::Preset));

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
            self.preferences(&ctx, &theme);
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
        if let Some(s) = self.screenshot.as_mut() {
            s.tick(&ctx);
        }
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
