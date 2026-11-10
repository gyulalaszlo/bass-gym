//! The results of a session: the dialog ([`results_dialog`]), the Statistics tab
//! ([`Stats`], a view) and the pieces exercises add to it ([`block`]).
//!
//! - `dialog.rs`: the modal. `stats.rs`: tiles, histograms and tables.
//! - `timeline.rs`: accuracy over time. `theme.rs`: how it all looks.

mod dialog;
mod stats;
mod theme;
mod timeline;

pub use dialog::{modal_open, results_dialog};
pub use stats::{Props, Stats, block};
pub use theme::{DialogTheme, Rates, TableTheme, Theme, TileTheme, TimelineTheme};

/// A tab of the results dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResultsTab {
    #[default]
    Stats,
    Review,
}

/// `m:ss` (or `h:mm:ss`).
pub fn format_duration(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn durations() {
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(61.6), "1:02");
        assert_eq!(format_duration(3725.0), "1:02:05");
    }
}
