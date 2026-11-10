//! Evaluation preferences of the Basic chords exercise.

use bg_core::GradeMode;
use bg_ui::egui::Color32;
use bg_ui::kit::{self, Emit, ThemeCache, View};
use bg_ui::theme::{Tokens, section};
use bg_ui::{PreferencePage, PrefsCtx, egui, icons};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasicChordsPrefs {
    pub grade_mode: GradeMode,
}

impl BasicChordsPrefs {
    pub const ID: &'static str = "basic_chords";
}

/// The Basic chords preference page: how chord fit counts in a grade.
#[derive(Default)]
pub struct BasicChordsPage {
    theme: ThemeCache<Theme>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub note: Color32,
    pub gap: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            note: t.dim(),
            gap: 4.0,
        }
    }
}

struct Grading;

impl View for Grading {
    type Props<'a> = BasicChordsPrefs;
    type Theme = Theme;
    type Emission = BasicChordsPrefs;

    fn view(
        p: &BasicChordsPrefs,
        theme: &Theme,
        ui: &mut egui::Ui,
        out: &mut Emit<Self::Emission>,
    ) {
        let note = |text: &str| egui::RichText::new(text).small().color(theme.note);
        let edit = kit::edited(p, |p| {
            section(ui, "Grading", |ui| {
                ui.radio_value(
                    &mut p.grade_mode,
                    GradeMode::Combined,
                    "Worst of timing and harmony",
                );
                ui.label(note(
                    "A note is only good when it's both on time and fits the chord.",
                ));
                ui.add_space(theme.gap);
                ui.radio_value(
                    &mut p.grade_mode,
                    GradeMode::Separate,
                    "Separate timing and harmony",
                );
                ui.label(note("Note colours and the hit rate cover timing only; harmony is shown as a dot above each note and in its own statistics."));
                ui.label(note("Applies to new sessions."));
            });
        });
        if let Some(p) = edit {
            out.send(p);
        }
    }
}

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
        let theme = self.theme.get();
        let p: BasicChordsPrefs = ctx.settings.get(BasicChordsPrefs::ID);
        for p in kit::show::<Grading>(ui, "grading", &p, &theme) {
            ctx.settings.set(BasicChordsPrefs::ID, &p);
        }
    }
}
