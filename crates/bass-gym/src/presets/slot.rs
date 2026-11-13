//! An exercise type with its presets: what's stored, what's selected, and whether the
//! working settings differ from it.

use std::path::{Path, PathBuf};

use bg_core::preset::{self, PresetEnvelope, PresetStore, StoredPreset, migrate};
use bg_ui::ExerciseType;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

/// An exercise type plus its preset state.
pub struct ExerciseSlot {
    pub et: Box<dyn ExerciseType>,
    pub presets: Vec<StoredPreset>,
    /// Every folder of every preset dir, as `(root, relative path)`.
    pub folders: Vec<(usize, PathBuf)>,
    /// The folder shown in the browser; new presets are saved here.
    pub folder: (usize, PathBuf),
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
            folders: Vec::new(),
            folder: (0, PathBuf::new()),
            current: None,
            baseline,
        };
        slot.refresh(store);
        slot
    }

    pub fn refresh(&mut self, store: &PresetStore) {
        let scan = store.scan(self.et.id(), self.et.as_ref());
        self.presets = scan.presets;
        self.folders = scan.folders;
        // A folder that vanished (or a removed dir) falls back to the user dir.
        let (root, dir) = &self.folder;
        let exists = *root < store.roots().len()
            && (dir.as_os_str().is_empty() || self.folders.iter().any(|f| f == &self.folder));
        if !exists {
            self.folder = (0, PathBuf::new());
        }
        // A selected preset that's gone leaves its settings as unsaved work.
        if self.current.is_some() && self.current_preset().is_none() {
            self.current = None;
            self.baseline = Value::Null;
        }
    }

    /// What changed since the last load or save, one line per setting.
    pub fn changes(&self) -> Vec<String> {
        preset::diff(&self.baseline, &self.et.save_preset())
    }

    pub fn is_dirty(&self) -> bool {
        self.et.save_preset() != self.baseline
    }

    pub fn current_preset(&self) -> Option<&StoredPreset> {
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

    /// Shows a folder in the browser.
    pub fn open_folder(&mut self, root: usize, folder: &Path) {
        self.folder = (root, folder.to_path_buf());
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
        let folder = (p.root, p.folder.clone());
        self.et.load_preset(data).map_err(|e| format!("{e:#}"))?;
        self.folder = folder;
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
        let path = store.new_path(self.folder.0, &self.folder.1, &env);
        preset::save(&path, &env).map_err(|e| e.to_string())?;
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
        preset::save(&p.path, &env).map_err(|e| e.to_string())?;
        self.refresh(store);
        self.baseline = self.et.save_preset();
        Ok(())
    }

    /// Changes the selected preset's name, description and tags, leaving its settings.
    pub fn edit(
        &mut self,
        store: &PresetStore,
        name: &str,
        description: &str,
        tags: Vec<String>,
    ) -> Result<(), String> {
        let Some(p) = self.current_preset() else {
            return Err("no preset selected".into());
        };
        let mut env = p.envelope.clone();
        env.name = name.to_owned();
        env.description = description.to_owned();
        env.tags = tags;
        env.modified = Utc::now();
        preset::save(&p.path, &env).map_err(|e| e.to_string())?;
        self.refresh(store);
        Ok(())
    }

    pub fn delete(&mut self, store: &PresetStore) -> Result<(), String> {
        let Some(p) = self.current_preset() else {
            return Err("no preset selected".into());
        };
        preset::delete(&p.path).map_err(|e| e.to_string())?;
        self.current = None;
        self.refresh(store);
        // Keep the working state; it is now unsaved.
        self.baseline = Value::Null;
        Ok(())
    }
}
