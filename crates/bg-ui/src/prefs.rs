//! The preference-page plug-in interface.

use bg_audio::AudioEngine;

use crate::settings::SettingsStore;

pub struct PrefsCtx<'a> {
    pub engine: &'a mut AudioEngine,
    pub settings: &'a mut SettingsStore,
}

/// A page of the preferences dialog. Pages keep their values in the [`SettingsStore`]
/// under their own id and apply side effects (e.g. reconfiguring audio) themselves.
pub trait PreferencePage {
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn icon(&self) -> &'static str;
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut PrefsCtx);
}
