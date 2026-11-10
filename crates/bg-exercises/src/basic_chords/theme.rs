//! The root of Basic chords' theme tree.

use bg_ui::egui::Color32;
use bg_ui::theme::{FormTheme, GradeColors, Tokens};
use bg_ui::views::{beatgrid, performance, results, review};

use super::view::{per_chord, status, strip};

pub struct Theme {
    pub grid: beatgrid::Theme,
    pub review: review::Theme,
    pub results: results::Theme,
    pub performance: performance::Theme,
    pub status: status::Theme,
    pub strip: strip::Theme,
    pub per_chord: per_chord::Theme,
    pub form: FormTheme,
    /// The bassist's note on the keyboard: by how it fit the chord, or `bass_unjudged`.
    pub bass: GradeColors,
    pub bass_unjudged: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub metrics: Metrics,
}

/// Space between the blocks of the exercise area, and the settings columns' width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Above the keyboard, below it and below the progression strip.
    pub top_gap: f32,
    pub keyboard_gap: f32,
    pub strip_gap: f32,
    /// Below the performance strip, below the grid and below the settings columns.
    pub meter_gap: f32,
    pub grid_gap: f32,
    pub columns_gap: f32,
    /// Narrowest a settings column gets before they stack.
    pub column_width: f32,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            grid: beatgrid::Theme::new(t),
            review: review::Theme::new(t),
            results: t.into(),
            performance: t.into(),
            status: status::Theme::new(t),
            strip: strip::Theme::new(t),
            per_chord: per_chord::Theme::new(t),
            form: FormTheme::new(t),
            bass: GradeColors::new(t),
            bass_unjudged: t.dim(),
            warning: t.amber(),
            error: t.red(),
            metrics: Metrics {
                top_gap: 6.0,
                keyboard_gap: 6.0,
                strip_gap: 6.0,
                meter_gap: 4.0,
                grid_gap: 8.0,
                columns_gap: 6.0,
                column_width: 330.0,
            },
        }
    }
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self::new(t)
    }
}
