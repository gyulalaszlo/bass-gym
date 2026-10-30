//! Evaluation preferences of the Basic chords exercise.

use bg_core::GradeMode;
use bg_ui::{PreferencePage, PrefsCtx, egui, icons, theme};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasicChordsPrefs {
    pub grade_mode: GradeMode,
}

impl BasicChordsPrefs {
    pub const ID: &'static str = "basic_chords";
}

pub struct BasicChordsPage;

impl PreferencePage for BasicChordsPage {
    fn id(&self) -> &'static str {
        BasicChordsPrefs::ID
    }
    fn title(&self) -> &'static str {
        "Basic chords"
    }
    fn icon(&self) -> &'static str {
        icons::PIANO_KEYS
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut PrefsCtx) {
        let mut p: BasicChordsPrefs = ctx.settings.get(BasicChordsPrefs::ID);
        theme::section(ui, "Grading", |ui| {
            ui.radio_value(&mut p.grade_mode, GradeMode::Combined, "Worst of timing and harmony");
            ui.label(egui::RichText::new("A note is only good when it's both on time and fits the chord.").small().color(theme::dim()));
            ui.add_space(4.0);
            ui.radio_value(&mut p.grade_mode, GradeMode::Separate, "Separate timing and harmony");
            ui.label(
                egui::RichText::new("Note colours and the hit rate cover timing only; harmony is shown as a dot above each note and in its own statistics.")
                    .small()
                    .color(theme::dim()),
            );
            ui.label(egui::RichText::new("Applies to new sessions.").small().color(theme::dim()));
        });
        ctx.settings.set(BasicChordsPrefs::ID, &p);
    }
}
