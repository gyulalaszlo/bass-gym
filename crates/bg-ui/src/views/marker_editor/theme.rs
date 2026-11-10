//! How the marker editor, and markers wherever they're drawn, look.

use egui::{Color32, FontId, Stroke};

use crate::theme::Tokens;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub wave_bg: Color32,
    pub wave_frame: Stroke,
    pub wave: Color32,
    pub playhead: Stroke,
    pub marker: MarkerTheme,
    pub label: LabelTheme,
    pub metrics: Metrics,
}

/// A marker's lines, span bar and dotted label. Colours come from each marker.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerTheme {
    pub stroke_width: f32,
    /// Height of a span's bar above the waveform.
    pub beam_height: f32,
    /// Opacity of a span's colour over the waveform, and of its bar.
    pub wave_dimming: f32,
    pub beam_dimming: f32,
    /// The stem from a point marker down to its label.
    pub stem: f32,
    pub stem_width: f32,
    /// Labels of dotted markers, along the top.
    pub font: FontId,
}

/// The text boxes under the waveform.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelTheme {
    pub font: FontId,
    pub fill: Color32,
    pub stroke_width: f32,
    pub rounding: u8,
    pub width: f32,
    pub min_height: f32,
    pub padding: f32,
    /// Room on a selected label for its delete button, and that button's height.
    pub button_width: f32,
    pub button_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Pointer distance that counts as being on a marker.
    pub grab_distance: f32,
    /// Room above the waveform for span bars.
    pub span_lane: f32,
    /// Between the waveform and the labels.
    pub label_gap: f32,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            wave_bg: t.bg(),
            wave_frame: Stroke::new(1.0, t.grid()),
            wave: t.text().gamma_multiply(0.65),
            playhead: Stroke::new(2.0, t.magenta()),
            marker: MarkerTheme::new(t),
            label: LabelTheme {
                font: t.mono(12.0),
                fill: t.panel(),
                stroke_width: 1.5,
                rounding: 6,
                width: t.em(240.0),
                min_height: t.em(40.0),
                padding: 6.0,
                button_width: 40.0,
                button_height: 24.0,
            },
            metrics: Metrics {
                grab_distance: 6.0,
                span_lane: 18.0,
                label_gap: 6.0,
            },
        }
    }
}

impl MarkerTheme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            stroke_width: 2.0,
            beam_height: 8.0,
            wave_dimming: 0.2,
            beam_dimming: 0.75,
            stem: 6.0,
            stem_width: 2.5,
            font: t.mono(10.0),
        }
    }
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self::new(t)
    }
}
