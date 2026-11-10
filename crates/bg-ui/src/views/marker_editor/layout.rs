//! Where the editor's parts go.

use egui::{Rangef, Rect, Ui};

use super::Props;
use super::state::{EditorState, Marker, WaveView};
use super::theme::Theme;
use crate::widgets::time_axis::TimeAxis;

/// The height of a marker's label, which grows with its text. Needs the fonts.
pub fn label_height(ui: &Ui, theme: &Theme, m: &Marker, selected: bool) -> f32 {
    let l = &theme.label;
    let wrap = l.width - 2.0 * l.padding - if selected { l.button_width } else { 0.0 };
    let galley = ui
        .painter()
        .layout(m.label.clone(), l.font.clone(), m.color, wrap);
    (galley.size().y + 2.0 * l.padding + 4.0).max(l.min_height)
}

/// Height of the area under the waveform: enough for the tallest label in view.
pub fn label_lane(ui: &Ui, theme: &Theme, p: &Props, state: &EditorState, width: f32) -> f32 {
    let view = state.view;
    let axis = TimeAxis::new(Rangef::new(0.0, width), view.start, view.end());
    let tallest = p
        .markers
        .iter()
        .filter(|m| (0.0..=width).contains(&axis.x(m.time)))
        .map(|m| label_height(ui, theme, m, state.selected == Some(m.id)))
        .fold(theme.label.min_height, f32::max);
    tallest + theme.metrics.label_gap + 4.0
}

pub struct Layout {
    /// Everything: header, span lane, waveform and labels.
    pub rect: Rect,
    pub wave: Rect,
    pub header: Option<Rect>,
    pub axis: TimeAxis,
}

/// `rect` holds a header of `header` points and labels `lane` points tall.
pub fn layout(rect: Rect, header: f32, lane: f32, span_lane: f32, view: WaveView) -> Layout {
    Layout {
        rect,
        wave: Rect::from_x_y_ranges(
            rect.x_range(),
            (rect.top() + header + span_lane)..=(rect.bottom() - lane),
        ),
        header: (header > 0.0).then(|| rect.with_max_y(rect.top() + header)),
        axis: TimeAxis::new(rect.x_range(), view.start, view.end()),
    }
}
