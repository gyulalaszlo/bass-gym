//! How the beat grid looks.

use egui::{Color32, FontId, Stroke, Vec2};

use crate::theme::{GradeColors, Tokens};

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub bg: Color32,
    pub frame: Stroke,
    pub bar: BarTheme,
    pub chord: ChordTheme,
    pub note: NoteTheme,
    /// The now line and its triangle.
    pub now: Color32,
    pub metrics: Metrics,
}

/// Sizes, margins and gaps. The ones that hold text follow the text size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Room for the bar header above the note lane.
    pub header: f32,
    /// Room below the note lane; the "click off" band fills it.
    pub footer: f32,
    /// Height of the chord band, when there are chords, and its top below the grid's.
    pub chord_band: f32,
    pub chord_band_top: f32,
    /// Bar header text from the bar line and the top.
    pub label_inset: Vec2,
    /// Between the bar number and the BPM, and between the BPM and its unit.
    pub label_gap: f32,
    pub unit_gap: f32,
    /// Chord name from the chord's left edge.
    pub chord_inset: f32,
    /// Expected notes, and accented ones.
    pub note_height: f32,
    pub accent_height: f32,
    /// Length of the bracket ends of a played note.
    pub bracket_tick: f32,
}

/// Bar lines, beat and step lines, and the header of each bar.
#[derive(Clone, Debug, PartialEq)]
pub struct BarTheme {
    pub line: Stroke,
    pub beat_line: Stroke,
    pub step_line: Stroke,
    /// Bar number, BPM and the count-in label.
    pub font: FontId,
    pub number: Color32,
    pub bpm: Color32,
    pub unit: Color32,
    pub count_in_fill: Color32,
    pub count_in_text: Color32,
    /// The "click off" band of a muted bar.
    pub muted_fill: Color32,
    pub muted_text: Color32,
    pub muted_font: FontId,
}

/// The chord band along the top.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordTheme {
    pub font: FontId,
    pub frame: Color32,
    pub text: Color32,
    /// Frame and text of the chord sounding now.
    pub active: Color32,
}

/// Expected and played notes.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteTheme {
    /// Outline of an expected note not yet decided.
    pub pending: Color32,
    /// Outline of an expected note sounding now.
    pub active: Color32,
    /// How strongly a decided note is filled with its grade colour.
    pub graded_fill: f32,
    pub grades: GradeColors,
    /// Played notes that matched nothing.
    pub extra: Color32,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            bg: t.bg(),
            frame: Stroke::new(1.0, t.grid()),
            bar: BarTheme {
                line: Stroke::new(2.0, t.line()),
                beat_line: Stroke::new(1.5, t.dim()),
                step_line: Stroke::new(1.0, t.grid()),
                font: t.mono(13.0),
                number: t.text(),
                bpm: t.green(),
                unit: t.cyan(),
                count_in_fill: t.raised(),
                count_in_text: t.amber(),
                muted_fill: t.amber().gamma_multiply(0.12),
                muted_text: t.amber(),
                muted_font: t.prop(12.0),
            },
            chord: ChordTheme {
                font: t.mono(13.0),
                frame: t.grid(),
                text: t.text(),
                active: t.cyan(),
            },
            note: NoteTheme {
                pending: t.text(),
                active: t.cyan(),
                graded_fill: 0.18,
                grades: GradeColors::new(t),
                extra: t.red(),
            },
            now: t.magenta(),
            metrics: Metrics::new(t),
        }
    }
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self::new(t)
    }
}

impl Metrics {
    pub fn new(t: &Tokens) -> Self {
        Self {
            header: t.em(30.0),
            footer: t.em(18.0),
            chord_band: t.em(24.0),
            chord_band_top: t.em(26.0),
            label_inset: t.em2(8.0, 6.0),
            label_gap: t.em(12.0),
            unit_gap: t.em(4.0),
            chord_inset: t.em(6.0),
            note_height: 20.0,
            accent_height: 30.0,
            bracket_tick: 6.0,
        }
    }
}
