//! How the results look.

use egui::{Color32, FontId, Stroke, Vec2, vec2};

use crate::theme::{GradeColors, Tokens};
use crate::views::histogram;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub dialog: DialogTheme,
    pub tile: TileTheme,
    /// A group of tiles: its frame, padding and spacing.
    pub group_frame: Stroke,
    pub group_margin: [i8; 2],
    pub group_spacing: Vec2,
    pub rates: Rates,
    pub grades: GradeColors,
    pub table: TableTheme,
    pub timeline: TimelineTheme,
    pub histogram: histogram::Theme,
    /// Histogram height, and the narrowest histogram column.
    pub histogram_height: f32,
    pub histogram_column: f32,
    /// The level histogram's bars.
    pub level: Color32,
    /// Between tile groups, and between tables.
    pub groups_gap: Vec2,
    pub tables_gap: Vec2,
    /// Between the rows of the statistics.
    pub section_gap: f32,
    /// Around a caption's block.
    pub block_spacing: Vec2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DialogTheme {
    pub margin: i8,
    pub min_width: f32,
    pub max_width: f32,
    /// With the Score + Audio tab.
    pub review_max_width: f32,
    /// Room kept around the dialog: either side, and above and below.
    pub screen_margin: f32,
    pub height_margin: f32,
    /// Taken by the title, tabs and Close button.
    pub chrome_height: f32,
    pub title_gap: f32,
    pub tabs_gap: f32,
    pub close_gap: f32,
    pub close_button: Vec2,
}

/// An ECAM readout: white label, coloured value, cyan unit.
#[derive(Clone, Debug, PartialEq)]
pub struct TileTheme {
    pub label_font: FontId,
    pub label: Color32,
    pub value_font: FontId,
    /// Values that aren't graded.
    pub value: Color32,
    pub unit_font: FontId,
    pub unit: Color32,
    pub spacing: Vec2,
}

/// Colours for a rate: good from 90%, ok from 70%, else bad; none without notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub good: Color32,
    pub ok: Color32,
    pub bad: Color32,
    pub none: Color32,
}

impl Rates {
    pub fn get(&self, rate: Option<f32>) -> Color32 {
        match rate {
            Some(r) if r >= 0.9 => self.good,
            Some(r) if r >= 0.7 => self.ok,
            Some(_) => self.bad,
            None => self.none,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableTheme {
    pub head: Color32,
    pub font: FontId,
    pub text: Color32,
    /// The extra notes' row.
    pub extra: Color32,
    pub spacing: Vec2,
}

/// The accuracy-over-time chart.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineTheme {
    pub height: f32,
    pub title_font: FontId,
    pub title: Color32,
    pub axis_font: FontId,
    pub axis: Stroke,
    pub label: Color32,
    /// The 100%, 90%, 70% and 50% guides, and how faint they are.
    pub guides: [Color32; 4],
    pub guide_alpha: f32,
    pub hover: Color32,
    pub trend: Stroke,
    pub tempo: Color32,
    pub tempo_line_alpha: f32,
    pub tooltip_font: FontId,
    /// Room left of the plot for the % labels, and above it for the tempo labels.
    pub left: f32,
    pub top: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            dialog: DialogTheme {
                margin: 20,
                min_width: 320.0,
                max_width: 960.0,
                review_max_width: 1400.0,
                screen_margin: 64.0,
                height_margin: 96.0,
                chrome_height: 110.0,
                title_gap: 6.0,
                tabs_gap: 8.0,
                close_gap: 8.0,
                close_button: vec2(96.0, 30.0),
            },
            tile: TileTheme {
                label_font: t.mono(13.0),
                label: t.text(),
                value_font: t.mono(22.0),
                value: t.text(),
                unit_font: t.mono(13.0),
                unit: t.cyan(),
                spacing: vec2(6.0, 2.0),
            },
            group_frame: Stroke::new(1.0, t.grid()),
            group_margin: [10, 8],
            group_spacing: vec2(18.0, 4.0),
            rates: Rates {
                good: t.green(),
                ok: t.amber(),
                bad: t.red(),
                none: t.dim(),
            },
            grades: GradeColors::new(t),
            table: TableTheme {
                head: t.dim(),
                font: t.mono(14.0),
                text: t.text(),
                extra: t.dim(),
                spacing: vec2(18.0, 2.0),
            },
            timeline: TimelineTheme {
                height: 150.0,
                title_font: t.mono(13.0),
                title: t.text(),
                axis_font: t.mono(11.0),
                axis: Stroke::new(1.0, t.line()),
                label: t.dim(),
                guides: [t.grid(), t.green(), t.amber(), t.grid()],
                guide_alpha: 0.4,
                hover: t.raised(),
                trend: Stroke::new(2.0, t.text()),
                tempo: t.cyan(),
                tempo_line_alpha: 0.6,
                tooltip_font: t.mono(13.0),
                left: 36.0,
                top: 22.0,
            },
            histogram: t.into(),
            histogram_height: t.em(130.0),
            histogram_column: 240.0,
            level: t.cyan(),
            groups_gap: vec2(12.0, 12.0),
            tables_gap: vec2(40.0, 16.0),
            section_gap: 12.0,
            block_spacing: vec2(8.0, 4.0),
        }
    }
}
