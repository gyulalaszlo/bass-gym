//! Scrolling beat grid: upcoming notes approach the "now" line from the right; played
//! notes are drawn over their targets in their grade colour as they pass. Optionally a
//! live score sits above the grid, on the same time axis.
//!
//! A tier-1 [`View`]: no state, and nothing to emit yet. It has no `input.rs` because
//! nothing in it is interactive.
//!
//! - `theme.rs`: how it looks, built from the tokens.
//! - `layout.rs`: where things go (pure, tested).
//! - `paint.rs`: drawing only.

mod layout;
mod paint;
mod theme;

use std::convert::Infallible;

use bg_core::harmony::Scale;
use bg_core::score::ScoreChord;
use bg_core::{BarPlan, Matcher};
use egui::{Sense, Ui};

use crate::kit::{Emit, View};
use crate::widgets::score::StaffLayout;

pub use layout::{Layout, layout};
pub use theme::{BarTheme, ChordTheme, NoteTheme, Theme};

/// The beat grid. Show it with [`crate::kit::show`].
pub struct Beatgrid;

pub struct Props<'a> {
    pub now: f64,
    pub bars: &'a [BarPlan],
    pub matcher: &'a Matcher,
    /// Grid subdivision drawn between beats.
    pub steps_per_beat: u32,
    /// Visible span in bars before / after the now line.
    pub bars_behind: f64,
    pub bars_ahead: f64,
    /// Optional chord band: (start, end, label) spans drawn along the top.
    pub chords: &'a [(f64, f64, String)],
    /// Show the played notes as a score above the grid.
    pub score: Option<LiveScore<'a>>,
    /// Height of the grid, without the score.
    pub height: f32,
    /// Divides the visible span (larger = zoomed in, so notes move slower).
    pub zoom: f64,
}

/// What the live score needs beyond the grid's own data.
#[derive(Clone, Copy)]
pub struct LiveScore<'a> {
    pub chords: &'a [ScoreChord],
    pub scale: Option<Scale>,
    /// Reference tuning, in Hz.
    pub a4: f32,
    /// Distance between staff lines.
    pub size: f32,
}

impl LiveScore<'_> {
    /// A fixed range, so the grid doesn't jump as notes come and go. It covers a
    /// five-string's low B and the 12th fret of the G string.
    pub fn layout(&self) -> StaffLayout {
        StaffLayout {
            spacing: self.size,
            lo: -5,
            hi: 14,
            labels: false,
        }
    }
}

impl View for Beatgrid {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(props: &Props, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        let size = layout::size(props, ui.available_width());
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        if ui.is_rect_visible(rect) {
            paint::paint(ui.painter(), props, theme, &layout(props, rect));
        }
    }
}
