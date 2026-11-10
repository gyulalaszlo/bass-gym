//! An exercise type with its presets: what's stored, what's selected, and whether the
//! working settings differ from it.

use bg_core::preset::{PresetEnvelope, PresetStore, StoredPreset, migrate};
use bg_ui::ExerciseType;
use chrono::Utc;
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

impl ExerciseSlot {
    pub fn new(et: Box<dyn ExerciseType>, store: &PresetStore) -> Self {
        let baseline = et.save_preset();
        let mut slot = Self {
            et,
            presets: Vec::new(),
            current: None,
            baseline,
        };
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
        self.current
            .and_then(|id| self.presets.iter().find(|p| p.envelope.id == id))
    }

    pub fn current_name(&self) -> String {
        self.current_preset()
            .map_or_else(|| "Default".to_owned(), |p| p.envelope.name.clone())
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
        let p = self
            .presets
            .iter()
            .find(|p| p.envelope.id == id)
            .ok_or("preset not found")?;
        let data = migrate(
            self.et.as_ref(),
            p.envelope.version,
            p.envelope.data.clone(),
        )
        .map_err(|e| e.to_string())?;
        self.et.load_preset(data).map_err(|e| format!("{e:#}"))?;
        self.current = Some(id);
        self.baseline = self.et.save_preset();
        Ok(())
    }

    /// Restores the working state (e.g. from the last run) on top of a selected preset.
    pub fn restore_working(&mut self, data: Value) {
        let data = migrate(self.et.as_ref(), self.et.current_version(), data);
        if let Ok(d) = data
            && let Err(e) = self.et.load_preset(d)
        {
            log::warn!("{}: couldn't restore working state: {e:#}", self.et.id());
        }
    }

    pub fn save_as(&mut self, store: &PresetStore, name: &str) -> Result<(), String> {
        let env = PresetEnvelope::new(
            self.et.id(),
            self.et.current_version(),
            name,
            self.et.save_preset(),
        );
        store.save(&env).map_err(|e| e.to_string())?;
        let id = env.id;
        self.refresh(store);
        self.current = Some(id);
        self.baseline = self.et.save_preset();
        Ok(())
    }

    pub fn save(&mut self, store: &PresetStore) -> Result<(), String> {
        let Some(p) = self.current_preset() else {
            return Err("no preset selected".into());
        };
        let mut env = p.envelope.clone();
        env.data = self.et.save_preset();
        env.version = self.et.current_version();
        env.modified = Utc::now();
        store.save(&env).map_err(|e| e.to_string())?;
        self.refresh(store);
        self.baseline = self.et.save_preset();
        Ok(())
    }

    pub fn rename(&mut self, store: &PresetStore, name: &str) -> Result<(), String> {
        let Some(p) = self.current_preset() else {
            return Err("no preset selected".into());
        };
        let mut env = p.envelope.clone();
        env.name = name.to_owned();
        env.modified = Utc::now();
        store.save(&env).map_err(|e| e.to_string())?;
        self.refresh(store);
        Ok(())
    }

    pub fn delete(&mut self, store: &PresetStore) -> Result<(), String> {
        let Some(p) = self.current_preset() else {
            return Err("no preset selected".into());
        };
        store.delete(&p.envelope).map_err(|e| e.to_string())?;
        self.current = None;
        self.refresh(store);
        // Keep the working state; it is now unsaved.
        self.baseline = Value::Null;
        Ok(())
    }
}
