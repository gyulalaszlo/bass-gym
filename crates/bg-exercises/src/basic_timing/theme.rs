//! The root of Basic timing's theme tree.

use bg_ui::egui::Color32;
use bg_ui::theme::Tokens;
use bg_ui::views::beatgrid;

use super::view::status;

pub struct Theme {
    pub grid: beatgrid::Theme,
    pub status: status::Theme,
    /// The "no audio output" warning.
    pub warning: Color32,
    /// Space below the performance strip, and below the grid.
    pub meter_gap: f32,
    pub grid_gap: f32,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            grid: beatgrid::Theme::new(t),
            status: status::Theme::new(t),
            warning: t.amber(),
            meter_gap: 4.0,
            grid_gap: 8.0,
        }
    }
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self::new(t)
    }
}
