//! Messages below the toolbar.

use bg_ui::egui::{RichText, Ui};
use bg_ui::icons;

use crate::basic_chords::props;
use crate::basic_chords::theme::Theme;

pub fn notices(p: &props::Notices, theme: &Theme, ui: &mut Ui) {
    if let Some(e) = p.error {
        ui.label(RichText::new(format!("{} {e}", icons::WARNING)).color(theme.error));
    }
    if p.no_audio {
        ui.label(
            RichText::new(format!(
                "{} No audio output — check Preferences › Audio",
                icons::WARNING
            ))
            .color(theme.warning),
        );
    }
}
