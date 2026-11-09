//! View preferences shared by all exercises: beat grid zoom and the live performance
//! indicator. Stored in the settings (not in presets), so changing them never marks a
//! preset as modified.

use serde::{Deserialize, Serialize};

/// Beat grid zoom levels, as a divisor of the visible span (larger = zoomed in, so notes
/// move slower).
pub const ZOOM_STEPS: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
/// Range of the score sizes: the distance between staff lines.
pub const SCORE_SIZE_RANGE: std::ops::RangeInclusive<f32> = 6.0..=24.0;
/// Index of the 1:1 zoom in [`ZOOM_STEPS`].
pub const ZOOM_DEFAULT: usize = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPrefs {
    /// Index into [`ZOOM_STEPS`].
    pub grid_zoom: usize,
    /// The performance indicator's range covers the last this many bars.
    pub indicator_bars: u32,
    /// Show played notes as a score above the beat grid.
    pub show_score: bool,
    /// Size of that score: the distance between staff lines.
    pub score_size: f32,
    /// Size of the score in the Results (Score + Audio).
    pub results_score_size: f32,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            grid_zoom: ZOOM_DEFAULT,
            indicator_bars: 2,
            show_score: true,
            score_size: 13.0,
            results_score_size: 8.0,
        }
    }
}

impl ViewPrefs {
    pub const ID: &'static str = "view";

    /// The span divisor of the current zoom level.
    pub fn zoom_factor(&self) -> f64 {
        ZOOM_STEPS[self.grid_zoom.min(ZOOM_STEPS.len() - 1)]
    }
}
