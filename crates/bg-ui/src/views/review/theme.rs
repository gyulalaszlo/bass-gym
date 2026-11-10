//! How the review looks.

use egui::{Color32, FontId, Stroke, Vec2, vec2};

use crate::theme::Tokens;
use crate::views::marker_editor;
use crate::widgets::score;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Selected bars, under the score and the waveform.
    pub selection: Color32,
    pub playhead: Stroke,
    pub wave_bg: Color32,
    pub wave_frame: Stroke,
    pub wave: Color32,
    /// Audio that was overwritten, and lost frames.
    pub overwritten: Color32,
    pub gap: Color32,
    /// Notes drawn on the waveform ("NO AUDIO RECORDED").
    pub label_font: FontId,
    pub label: Color32,
    /// The playback position (bar, beat, 32nd): values and units.
    pub position_font: FontId,
    pub position: Color32,
    pub unit_font: FontId,
    pub unit: Color32,
    /// Hints, and the export's progress, failure and result.
    pub hint: Color32,
    pub failed: Color32,
    pub done: Color32,
    /// The marker editor, and markers on the score rows.
    pub editor: marker_editor::Theme,
    /// Markers the user adds.
    pub new_marker: Color32,
    pub score: score::Theme,
    pub metrics: Metrics,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub wave_height: f32,
    /// Room for the note bars above the waveform.
    pub note_lane: f32,
    pub row_gap: f32,
    pub wave_rounding: f32,
    pub editor_height: f32,
    pub play_button: Vec2,
    pub bar_field_width: f32,
    pub title_width: f32,
    pub export_button: Vec2,
    pub notes_min_width: f32,
    /// Between groups in the controls row, and before the export button.
    pub group_gap: f32,
    /// Below the controls, below the report row, and above the marker editor.
    pub controls_gap: f32,
    pub report_gap: f32,
    pub editor_gap: f32,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            selection: t.cyan().gamma_multiply(0.1),
            playhead: Stroke::new(2.0, t.magenta()),
            wave_bg: t.bg(),
            wave_frame: Stroke::new(1.0, t.grid()),
            wave: t.text().gamma_multiply(0.65),
            overwritten: t.raised(),
            gap: t.amber().gamma_multiply(0.4),
            label_font: t.mono(11.0),
            label: t.dim(),
            position_font: t.mono(16.0),
            position: t.text(),
            unit_font: t.mono(10.0),
            unit: t.cyan(),
            hint: t.dim(),
            failed: t.red(),
            done: t.green(),
            editor: marker_editor::Theme::new(t),
            new_marker: t.cyan(),
            score: t.into(),
            metrics: Metrics {
                wave_height: 60.0,
                note_lane: 10.0,
                row_gap: 18.0,
                wave_rounding: 4.0,
                editor_height: 260.0,
                play_button: vec2(84.0, 30.0),
                bar_field_width: 44.0,
                title_width: 220.0,
                export_button: vec2(190.0, 30.0),
                notes_min_width: 120.0,
                group_gap: 12.0,
                controls_gap: 6.0,
                report_gap: 10.0,
                editor_gap: 6.0,
            },
        }
    }
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self::new(t)
    }
}
