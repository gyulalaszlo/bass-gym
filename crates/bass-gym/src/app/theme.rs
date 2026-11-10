//! The root of the shell's theme tree. Exercises keep theirs.

use bg_ui::egui::{Color32, Stroke};
use bg_ui::theme::Tokens;

use super::{prefs, sidebar};
use crate::presets::bar;

pub struct Theme {
    pub sidebar: sidebar::Theme,
    pub prefs: prefs::Theme,
    pub presets: bar::Theme,
    pub sidebar_fill: Color32,
    pub sidebar_frame: Stroke,
    pub sidebar_width: f32,
    pub sidebar_margin: [i8; 2],
    pub panel_fill: Color32,
    pub panel_margin: i8,
    /// Below the tuner, and below the preset bar.
    pub tuner_gap: f32,
    pub presets_gap: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            sidebar: t.into(),
            prefs: t.into(),
            presets: t.into(),
            sidebar_fill: t.bg(),
            sidebar_frame: Stroke::new(1.0, t.grid()),
            sidebar_width: 240.0,
            sidebar_margin: [12, 4],
            panel_fill: t.panel(),
            panel_margin: 14,
            tuner_gap: 8.0,
            presets_gap: 2.0,
        }
    }
}
