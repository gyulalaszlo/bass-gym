//! Drawing the editor and its markers. Reads only props, state, theme and layout.

use egui::{Align2, Painter, Rect, Stroke, StrokeKind, pos2};

use super::Props;
use super::layout::Layout;
use super::state::{EditorState, Line, Marker};
use super::theme::{MarkerTheme, Theme};
use crate::widgets::score::dotted_vline;
use crate::widgets::time_axis::TimeAxis;
use crate::widgets::waveform::{self, WaveStyle};

/// The waveform, the header, the markers and the playhead.
pub fn paint(p: &Painter, props: &Props, state: &EditorState, theme: &Theme, l: &Layout) {
    p.rect_filled(l.wave, 0.0, theme.wave_bg);
    p.rect_stroke(l.wave, 0.0, theme.wave_frame, StrokeKind::Inside);
    waveform::paint(
        p,
        l.wave,
        l.axis,
        props.source,
        WaveStyle {
            log: state.log,
            color: theme.wave,
        },
    );
    if let (Some(h), Some(rect)) = (&props.header, l.header) {
        (h.paint)(p, rect, l.axis);
    }
    for m in props.overlays.iter().chain(props.markers) {
        paint_marker(p, &theme.marker, l.axis, m, l.rect, l.wave);
    }
    if let Some(t) = props.playhead {
        p.vline(l.axis.x(t), l.rect.y_range(), theme.playhead);
    }
}

/// Paints a marker over `wave`. Dotted lines reach up to the top of `rect`, so they can
/// run through whatever sits above the waveform.
pub fn paint_marker(
    p: &egui::Painter,
    t: &MarkerTheme,
    axis: TimeAxis,
    m: &Marker,
    rect: Rect,
    wave: Rect,
) {
    let x = axis.x(m.time);
    match m.line {
        Line::Solid => {
            p.vline(x, wave.y_range(), Stroke::new(t.stroke_width, m.color));
        }
        Line::Dotted => dotted_vline(p, x, rect.top()..=wave.bottom(), m.color),
    }
    if let Some(end) = m.end {
        let x1 = axis.x(end);

        let beam_top = wave.top() - t.beam_height;
        let full_vertical_range = beam_top..=wave.bottom();
        let connecting_beam_vertical_range = beam_top..=wave.top();
        let horizontal_range = x..=x1;

        // paint the connecting beam BG
        p.rect_filled(
            Rect::from_x_y_ranges(&horizontal_range, connecting_beam_vertical_range),
            0.0,
            m.color.gamma_multiply(t.beam_dimming),
        );

        // paint an overlay with the marker colour above the waveform
        p.rect_filled(
            Rect::from_x_y_ranges(&horizontal_range, wave.y_range()),
            0.0,
            m.color.gamma_multiply(t.wave_dimming),
        );

        // paint the stroke around the marker
        let marker_full_area_rect = Rect::from_x_y_ranges(x..=x1, full_vertical_range);

        p.line(
            vec![
                marker_full_area_rect.left_bottom(),
                marker_full_area_rect.left_top(),
                marker_full_area_rect.right_top(),
                marker_full_area_rect.right_bottom(),
            ],
            Stroke::new(t.stroke_width, m.color),
        );
    }
    if m.line == Line::Solid && m.end.is_none() {
        // A stem down to the label.
        p.vline(
            x,
            wave.bottom()..=wave.bottom() + t.stem,
            Stroke::new(t.stem_width, m.color),
        );
    }
    if !m.label.is_empty() && m.line == Line::Dotted {
        p.text(
            pos2(x + 3.0, rect.top()),
            Align2::LEFT_TOP,
            &m.label,
            t.font.clone(),
            m.color,
        );
    }
}
