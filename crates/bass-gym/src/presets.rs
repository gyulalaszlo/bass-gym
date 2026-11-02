//! Preset selection / creation for the active exercise type.

use bg_core::preset::{PresetEnvelope, PresetStore, StoredPreset, migrate};
use bg_ui::widgets::button::{button, selectable};
use bg_ui::widgets::controls::dropdown_sized;
use bg_ui::{ExerciseType, egui, icons, theme};
use chrono::Utc;
use egui::RichText;
use serde_json::Value;
use uuid::Uuid;

/// An exercise type plus its preset state.
pub struct ExerciseSlot {
    pub et: Box<dyn ExerciseType>,
    pub presets: Vec<StoredPreset>,
    /// Selected stored preset; `None` = factory default.
    pub current: Option<Uuid>,
    /// Data as last loaded/saved; the working state is dirty when it differs.
    pub baseline: Value,
}

enum Dialog {
    New(String),
    Rename(String),
    Delete,
}

#[derive(Default)]
pub struct PresetBar {
    dialog: Option<Dialog>,
    pub message: Option<(String, bool)>,
}

impl ExerciseSlot {
    pub fn new(et: Box<dyn ExerciseType>, store: &PresetStore) -> Self {
        let baseline = et.save_preset();
        let mut slot = Self { et, presets: Vec::new(), current: None, baseline };
        slot.refresh(store);
        slot
    }

    pub fn refresh(&mut self, store: &PresetStore) {
        self.presets = store.list(self.et.id(), self.et.as_ref());
    }

    pub fn is_dirty(&self) -> bool {
        self.et.save_preset() != self.baseline
    }

    fn current_preset(&self) -> Option<&StoredPreset> {
        self.current.and_then(|id| self.presets.iter().find(|p| p.envelope.id == id))
    }

    pub fn current_name(&self) -> String {
        self.current_preset().map_or_else(|| "Default".to_owned(), |p| p.envelope.name.clone())
    }

    /// Loads the factory default.
    pub fn load_default(&mut self) {
        let data = self.et.default_preset();
        if let Err(e) = self.et.load_preset(data) {
            log::error!("{}: default preset failed to load: {e:#}", self.et.id());
        }
        self.current = None;
        self.baseline = self.et.save_preset();
    }

    pub fn load(&mut self, id: Uuid) -> Result<(), String> {
        let p = self.presets.iter().find(|p| p.envelope.id == id).ok_or("preset not found")?;
        let data = migrate(self.et.as_ref(), p.envelope.version, p.envelope.data.clone()).map_err(|e| e.to_string())?;
        self.et.load_preset(data).map_err(|e| format!("{e:#}"))?;
        self.current = Some(id);
        self.baseline = self.et.save_preset();
        Ok(())
    }

    /// Restores the working state (e.g. from the last run) on top of a selected preset.
    pub fn restore_working(&mut self, data: Value) {
        let data = migrate(self.et.as_ref(), self.et.current_version(), data);
        if let Ok(d) = data
            && let Err(e) = self.et.load_preset(d) {
                log::warn!("{}: couldn't restore working state: {e:#}", self.et.id());
            }
    }

    fn save_as(&mut self, store: &PresetStore, name: &str) -> Result<(), String> {
        let env = PresetEnvelope::new(self.et.id(), self.et.current_version(), name, self.et.save_preset());
        store.save(&env).map_err(|e| e.to_string())?;
        let id = env.id;
        self.refresh(store);
        self.current = Some(id);
        self.baseline = self.et.save_preset();
        Ok(())
    }

    fn save(&mut self, store: &PresetStore) -> Result<(), String> {
        let Some(p) = self.current_preset() else { return Err("no preset selected".into()) };
        let mut env = p.envelope.clone();
        env.data = self.et.save_preset();
        env.version = self.et.current_version();
        env.modified = Utc::now();
        store.save(&env).map_err(|e| e.to_string())?;
        self.refresh(store);
        self.baseline = self.et.save_preset();
        Ok(())
    }

    fn rename(&mut self, store: &PresetStore, name: &str) -> Result<(), String> {
        let Some(p) = self.current_preset() else { return Err("no preset selected".into()) };
        let mut env = p.envelope.clone();
        env.name = name.to_owned();
        env.modified = Utc::now();
        store.save(&env).map_err(|e| e.to_string())?;
        self.refresh(store);
        Ok(())
    }

    fn delete(&mut self, store: &PresetStore) -> Result<(), String> {
        let Some(p) = self.current_preset() else { return Err("no preset selected".into()) };
        store.delete(&p.envelope).map_err(|e| e.to_string())?;
        self.current = None;
        self.refresh(store);
        // Keep the working state; it is now unsaved.
        self.baseline = Value::Null;
        Ok(())
    }
}

impl PresetBar {
    pub fn ui(&mut self, ui: &mut egui::Ui, slot: &mut ExerciseSlot, store: &PresetStore) {
        let locked = slot.et.is_running();
        let dirty = slot.is_dirty();
        let mut result: Option<Result<(), String>> = None;

        // The selector takes the space the buttons leave (measured on the previous frame),
        // so the buttons stay at the right edge.
        let buttons_id = ui.id().with("preset_buttons_width");
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!locked, |ui| {
                let label = format!("{}  {}{}", icons::BOOKMARK_SIMPLE, slot.current_name(), if dirty { "  •" } else { "" });
                let text = RichText::new(label).color(if dirty { theme::amber() } else { theme::cyan() });
                let mut pick: Option<Option<Uuid>> = None;
                let buttons_w: f32 = ui.data(|d| d.get_temp(buttons_id)).unwrap_or(560.0);
                let width = (ui.available_width() - buttons_w - ui.spacing().item_spacing.x).max(140.0);
                dropdown_sized(ui, "preset_select", text, width, |ui| {
                    if ui.add(selectable(slot.current.is_none(), "Default")).clicked() {
                        pick = Some(None);
                    }
                    for p in &slot.presets {
                        let selected = slot.current == Some(p.envelope.id);
                        match &p.incompatible {
                            None => {
                                if ui.add(selectable(selected, &p.envelope.name)).clicked() {
                                    pick = Some(Some(p.envelope.id));
                                }
                            }
                            Some(why) => {
                                ui.add_enabled(false, selectable(false, format!("{} {}", icons::WARNING, p.envelope.name))).on_disabled_hover_text(why);
                            }
                        }
                    }
                });
                if let Some(choice) = pick {
                    match choice {
                        None => slot.load_default(),
                        Some(id) => result = Some(slot.load(id)),
                    }
                }

                let buttons = ui.horizontal(|ui| {
                let can_save = dirty && slot.current.is_some();
                if ui.add_enabled(can_save, button(format!("{} Save", icons::FLOPPY_DISK))).on_hover_text("Overwrite the selected preset").clicked() {
                    result = Some(slot.save(store));
                }
                if ui.add(button(format!("{} New", icons::PLUS))).on_hover_text("Save the current settings as a new preset").clicked() {
                    self.dialog = Some(Dialog::New(String::new()));
                }
                if ui.add_enabled(dirty, button(format!("{} Revert", icons::ARROW_COUNTER_CLOCKWISE))).on_hover_text("Discard unsaved changes").clicked() {
                    match slot.current {
                        Some(id) => result = Some(slot.load(id)),
                        None => slot.load_default(),
                    }
                }
                if ui.add(button(format!("{} Defaults", icons::ARROW_U_UP_LEFT))).on_hover_text("Reset the settings to the factory defaults (keeps the preset selected)").clicked() {
                    let data = slot.et.default_preset();
                    result = Some(slot.et.load_preset(data).map_err(|e| format!("{e:#}")));
                }
                ui.add_enabled_ui(slot.current.is_some(), |ui| {
                    let more = ui.add(button(icons::DOTS_THREE)).on_hover_text("More preset actions");
                    egui::Popup::menu(&more).show(|ui| {
                        if ui.add(button(format!("{} Rename…", icons::PENCIL_SIMPLE)).min_size([ui.available_width(), 0.0])).clicked() {
                            self.dialog = Some(Dialog::Rename(slot.current_name()));
                        }
                        if ui.add(button(format!("{} Delete…", icons::TRASH)).color(theme::red()).min_size([ui.available_width(), 0.0])).clicked() {
                            self.dialog = Some(Dialog::Delete);
                        }
                    });
                });
                });
                let w = buttons.response.rect.width();
                if (w - buttons_w).abs() > 0.5 {
                    ui.data_mut(|d| d.insert_temp(buttons_id, w));
                    ui.ctx().request_repaint();
                }
            });
        });
        if let Some((msg, ok)) = &self.message {
            ui.label(RichText::new(msg).small().color(if *ok { theme::green() } else { theme::red() }));
        }

        if let Some(dialog) = &mut self.dialog {
            let mut close = false;
            egui::Modal::new(egui::Id::new("preset_dialog")).show(ui.ctx(), |ui| {
                ui.set_width(340.0);
                let is_new = matches!(dialog, Dialog::New(_));
                match dialog {
                    Dialog::New(name) | Dialog::Rename(name) => {
                        ui.heading(if is_new { "New preset" } else { "Rename preset" });
                        let resp = ui.text_edit_singleline(name);
                        resp.request_focus();
                        let valid = !name.trim().is_empty();
                        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        ui.horizontal(|ui| {
                            if ui.add_enabled(valid, button("Save")).clicked() || (enter && valid) {
                                let name = name.trim().to_owned();
                                result = Some(if is_new { slot.save_as(store, &name) } else { slot.rename(store, &name) });
                                close = true;
                            }
                            if ui.add(button("Cancel")).clicked() {
                                close = true;
                            }
                        });
                    }
                    Dialog::Delete => {
                        ui.heading("Delete preset");
                        ui.label(format!("Delete “{}”? This can't be undone.", slot.current_name()));
                        ui.horizontal(|ui| {
                            if ui.add(button("Delete").color(theme::red())).clicked() {
                                result = Some(slot.delete(store));
                                close = true;
                            }
                            if ui.add(button("Cancel")).clicked() {
                                close = true;
                            }
                        });
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
            if close {
                self.dialog = None;
            }
        }

        if let Some(r) = result {
            self.message = match r {
                Ok(()) => None,
                Err(e) => Some((e, false)),
            };
        }
    }
}
