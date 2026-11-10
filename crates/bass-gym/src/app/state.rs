//! What survives a restart besides settings and presets: the selected exercise and
//! presets, the unsaved working settings, and the preferences window's geometry.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Kept as JSON next to the settings.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub selected: Option<String>,
    pub presets: HashMap<String, Uuid>,
    pub working: HashMap<String, Value>,
    /// Preferences window position and size (logical points).
    pub prefs_window: Option<WindowGeometry>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub pos: [f32; 2],
    pub size: [f32; 2],
}

impl AppState {
    pub fn load(path: Option<&Path>) -> Self {
        path.and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self)
            && let Err(e) = std::fs::write(path, text)
        {
            log::warn!("couldn't save app state: {e}");
        }
    }
}
