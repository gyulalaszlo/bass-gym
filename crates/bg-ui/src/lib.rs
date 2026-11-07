//! Shared UI for bass-gym: the ECAM-inspired theme, the exercise-type and
//! preference-page plug-in interfaces, settings storage and reusable widgets.

pub mod exercise;
pub mod prefs;
pub mod recording;
pub mod settings;
pub mod theme;
pub mod view;
pub mod widgets;

pub use egui;
pub use egui_phosphor::regular as icons;
pub use exercise::{ExerciseCtx, ExerciseType};
pub use prefs::{PreferencePage, PrefsCtx};
pub use settings::SettingsStore;
