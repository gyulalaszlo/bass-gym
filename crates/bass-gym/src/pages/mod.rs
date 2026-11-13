//! Built-in preference pages, one per file (the Audio page is a folder).
//!
//! Each page follows the kit pattern for settings: a section is a view over the settings
//! struct it edits (its props) and emits the edited struct, built with `kit::edited`.
//! The page's `ui` is the glue: it reads the settings, shows the sections, and for each
//! change stores it and makes the engine call that applies it, only when something
//! changed. Page-local state (a calibration, a device list) lives on the page struct.

mod audio;
mod display;
mod latency;
pub mod presets;
mod scoring;
mod tuner;

use bg_ui::PreferencePage;
use bg_ui::egui::{self, Color32, FontId, RichText, Ui, Vec2, vec2};
use bg_ui::theme::{FormTheme, Tokens};

pub use bg_ui::settings::{ANALYSIS, AUDIO, SOUNDS};
pub const SCORING: &str = "scoring";
pub const PRESETS: &str = "presets";

pub fn builtin() -> Vec<Box<dyn PreferencePage>> {
    vec![
        Box::new(display::DisplayPage::default()),
        Box::new(audio::AudioPage::default()),
        Box::new(latency::LatencyPage::default()),
        Box::new(tuner::TunerPage::default()),
        Box::new(scoring::ScoringPage::default()),
    ]
}

/// How the preference pages look.
#[derive(Clone, Debug, PartialEq)]
pub struct PageTheme {
    pub form: FormTheme,
    pub ok: Color32,
    pub warning: Color32,
    pub error: Color32,
    /// Explanations under controls.
    pub note: Color32,
    /// Units and selectable values.
    pub unit: Color32,
    /// Large readouts (zoom %, measured latency) and their units.
    pub value_font: FontId,
    pub unit_font: FontId,
    pub indicator_font: FontId,
    /// Label / control grids, and the tighter one of the detector settings.
    pub grid_spacing: Vec2,
    pub tight_grid_spacing: Vec2,
    /// Between sections, and between groups within one.
    pub section_gap: f32,
    pub gap: f32,
}

impl PageTheme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            form: FormTheme::new(t),
            ok: t.green(),
            warning: t.amber(),
            error: t.red(),
            note: t.dim(),
            unit: t.cyan(),
            value_font: t.mono(16.0),
            unit_font: t.mono(13.0),
            indicator_font: t.mono(12.0),
            grid_spacing: vec2(12.0, 8.0),
            tight_grid_spacing: vec2(12.0, 6.0),
            section_gap: 6.0,
            gap: 4.0,
        }
    }

    /// A small note in the note colour.
    pub fn note(&self, text: impl Into<String>) -> RichText {
        RichText::new(text).small().color(self.note)
    }
}

impl From<&Tokens> for PageTheme {
    fn from(t: &Tokens) -> Self {
        Self::new(t)
    }
}

/// A small note that wraps.
fn help(ui: &mut Ui, theme: &PageTheme, text: &str) {
    ui.add(egui::Label::new(theme.note(text)).wrap());
}

/// A label / control grid with the page's spacing.
fn grid(id: &str, theme: &PageTheme) -> egui::Grid {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(theme.grid_spacing)
}
