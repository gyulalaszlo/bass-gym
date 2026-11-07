//! The exercise-type plug-in interface.

use std::path::Path;

use anyhow::Result;
use bg_audio::AudioEngine;
use bg_core::ScoringThresholds;
use bg_core::preset::PresetSchema;
use serde_json::Value;

use crate::prefs::PreferencePage;
use crate::settings::SettingsStore;
use crate::widgets::toolbar::Toolbar;

/// Everything an exercise UI may use while drawing.
pub struct ExerciseCtx<'a> {
    pub engine: &'a mut AudioEngine,
    pub settings: &'a mut SettingsStore,
    pub scoring: &'a ScoringThresholds,
    /// Where reports and other user data go, if there's such a place.
    pub data_dir: Option<&'a Path>,
}

/// An exercise type: its UI, logic and preset data.
///
/// Presets hold only the exercise's own configuration (never results). The data is
/// versioned through [`PresetSchema`]: bump `current_version` when the data shape
/// changes incompatibly and implement `migrate_step`. Additive changes only need
/// `#[serde(default)]` on the data structs.
pub trait ExerciseType: PresetSchema {
    /// Stable identifier, used for preset folders and settings. Never change it.
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn icon(&self) -> &'static str;
    fn description(&self) -> &'static str {
        ""
    }

    /// The factory default preset data (current version).
    fn default_preset(&self) -> Value;
    /// Current configuration as preset data (current version).
    fn save_preset(&self) -> Value;
    /// Applies preset data that was already migrated to the current version.
    fn load_preset(&mut self, data: Value) -> Result<()>;

    /// Whether a session is running; presets can't be switched meanwhile.
    fn is_running(&self) -> bool {
        false
    }

    /// Fills the Exercise Control Toolbar, shown above the (scrolling) exercise area.
    /// It should at least hold the start / stop button ([`Toolbar::transport`]).
    fn toolbar(&mut self, tb: &mut Toolbar, ctx: &mut ExerciseCtx);

    /// Debugging aid: runs a made-up session offline and opens its review, so that it
    /// can be looked at without playing.
    fn debug_fake_session(&mut self, _ctx: &mut ExerciseCtx) {}

    /// Messages shown just below the toolbar (errors, warnings); none by default.
    fn notices(&mut self, _ui: &mut egui::Ui, _ctx: &mut ExerciseCtx) {}

    /// The exercise area (scrolls).
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut ExerciseCtx);

    /// Extra preference pages contributed by this exercise type.
    fn preference_pages(&self) -> Vec<Box<dyn PreferencePage>> {
        Vec::new()
    }
}
