//! A zoomable waveform with markers on it. It knows nothing about where the audio or
//! markers come from: the owner provides a [`PeakSource`], read-only overlays and the
//! editable [`Marker`]s, so the same editor serves session reports and loop points.
//!
//! Gestures: dragging holds on to the point it started on, moving it with the pointer
//! while vertical movement zooms around it (down zooms in), so a single drag can do both.
//! Pinch or Ctrl/Cmd + wheel zoom too, and scrolling sideways pans. Drag an editable
//! marker to move it, double-click to add one, click one to select it and edit its text.

use egui::{Align2, Color32, CornerRadius, Key, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::theme::{self, mono};
use crate::widgets::button::button;
use crate::widgets::score::dotted_vline;
use crate::widgets::time_axis::TimeAxis;
use crate::widgets::waveform::{self, PeakSource, WaveStyle};

/// Pointer distance that counts as being on a marker.
const GRAB_DISTANCE: f32 = 6.0;
const SPAN_LANE: f32 = 18.0;
/// Gap between the waveform and the labels below it.
const LABEL_GAP: f32 = 6.0;
const LABEL_WIDTH: f32 = 240.0;
const LABEL_MIN_HEIGHT: f32 = 40.0;
const LABEL_PADDING: f32 = 6.0;
/// Room on a selected label for its delete button.
const LABEL_BUTTON: f32 = 40.0;
/// Smallest visible span, in axis units.
const MIN_SPAN: f64 = 0.005;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Solid,
    Dotted,
}

/// A point or span on the time axis (seconds, in whatever time base the owner uses).
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub id: u64,
    pub time: f64,
    /// Set for a span, drawn as a bar along the top.
    pub end: Option<f64>,
    pub label: String,
    pub color: Color32,
    pub line: Line,
}

/// The visible range of the editor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveView {
    pub start: f64,
    pub span: f64,
    bounds: (f64, f64),
}

impl WaveView {
    /// Shows `start..end`, within the `bounds` the view may move in.
    pub fn new(start: f64, end: f64, bounds: (f64, f64)) -> Self {
        let mut v = Self {
            start,
            span: end - start,
            bounds,
        };
        v.clamp();
        v
    }

    pub fn end(&self) -> f64 {
        self.start + self.span
    }

    /// Scales the span by `factor`, keeping time `at` in place.
    pub fn zoom_at(&mut self, at: f64, factor: f64) {
        let old = self.span;
        self.span = (old * factor).clamp(MIN_SPAN, self.max_span());
        self.start = at - (at - self.start) * self.span / old;
        self.clamp();
    }

    /// Scales the span by `factor` and puts time `at` at `fraction` of the view's width
    /// (0 = left edge), so a drag can zoom and move at once.
    pub fn navigate(&mut self, at: f64, fraction: f64, factor: f64) {
        self.span = (self.span * factor).clamp(MIN_SPAN, self.max_span());
        self.start = at - fraction * self.span;
        self.clamp();
    }

    pub fn pan(&mut self, by: f64) {
        self.start += by;
        self.clamp();
    }

    fn max_span(&self) -> f64 {
        (self.bounds.1 - self.bounds.0).max(MIN_SPAN)
    }

    fn clamp(&mut self) {
        self.span = self.span.clamp(MIN_SPAN, self.max_span());
        self.start = self.start.min(self.bounds.1 - self.span).max(self.bounds.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    Marker(u64),
    /// Dragging the view by the point at time `at`.
    Navigate {
        at: f64,
    },
}

/// What the editor remembers between frames. Owned by whoever shows it.
#[derive(Clone, Debug)]
pub struct EditorState {
    pub view: WaveView,
    pub selected: Option<u64>,
    /// Show levels on a log scale.
    pub log: bool,
    gesture: Option<Gesture>,
}

impl EditorState {
    pub fn new(view: WaveView) -> Self {
        Self {
            view,
            selected: None,
            log: false,
            gesture: None,
        }
    }
}

pub struct MarkerEditor<'a> {
    pub source: &'a dyn PeakSource,
    /// Drawn but not editable.
    pub overlays: &'a [Marker],
    pub markers: &'a mut Vec<Marker>,
    pub state: &'a mut EditorState,
    /// Adjusts where a marker lands when placed or moved.
    pub snap: &'a dyn Fn(f64) -> f64,
    /// Double-click adds markers, and they can be deleted. Off when the owner has a fixed
    /// set, such as loop points.
    pub add_remove: bool,
    /// Creates a new marker at a time.
    pub new_marker: &'a mut dyn FnMut(f64) -> Marker,
    pub playhead: Option<f64>,
}

/// The marker within reach of screen position `x`, nearest first.
fn marker_near(markers: &[Marker], axis: TimeAxis, x: f32) -> Option<u64> {
    markers
        .iter()
        .map(|m| (m.id, (axis.x(m.time) - x).abs()))
        .filter(|&(_, d)| d <= GRAB_DISTANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Zoom factor of a vertical drag: dragging down zooms in.
fn drag_zoom(dy: f32) -> f64 {
    (-dy as f64 * 0.01).exp()
}

/// The font of marker labels.
fn label_font() -> egui::FontId {
    mono(12.0)
}

impl MarkerEditor<'_> {
    /// `height` is that of the waveform and everything above it; the labels below take
    /// what their text needs.
    pub fn show(mut self, ui: &mut egui::Ui, height: f32) {
        let width = ui.available_width();
        let view = self.state.view;
        let lane = self.label_lane(ui, width, view);
        let (rect, resp) =
            ui.allocate_exact_size(vec2(width, height + lane), Sense::click_and_drag());
        let wave = Rect::from_x_y_ranges(
            rect.x_range(),
            (rect.top() + SPAN_LANE)..=(rect.bottom() - lane),
        );
        let axis = TimeAxis::new(rect.x_range(), view.start, view.end());

        self.handle_input(ui, rect, &resp, axis);
        let view = self.state.view;
        let axis = TimeAxis::new(rect.x_range(), view.start, view.end());

        let p = ui.painter_at(rect);
        p.rect_filled(wave, 0.0, theme::bg());
        p.rect_stroke(
            wave,
            0.0,
            Stroke::new(1.0, theme::grid()),
            StrokeKind::Inside,
        );
        waveform::paint(
            &p,
            wave,
            axis,
            self.source,
            WaveStyle {
                log: self.state.log,
                color: theme::text().gamma_multiply(0.65),
            },
        );
        for m in self.overlays {
            paint_marker(&p, axis, m, rect, wave);
        }
        for m in self.markers.iter() {
            paint_marker(&p, axis, m, rect, wave);
        }
        if let Some(t) = self.playhead {
            p.vline(
                axis.x(t),
                rect.y_range(),
                Stroke::new(2.0, theme::magenta()),
            );
        }
        self.labels(ui, rect, wave, axis);
    }

    /// The height of a marker's label, which grows with its text.
    fn label_height(ui: &egui::Ui, m: &Marker, selected: bool) -> f32 {
        let wrap = LABEL_WIDTH - 2.0 * LABEL_PADDING - if selected { LABEL_BUTTON } else { 0.0 };
        let galley = ui
            .painter()
            .layout(m.label.clone(), label_font(), m.color, wrap);
        (galley.size().y + 2.0 * LABEL_PADDING + 4.0).max(LABEL_MIN_HEIGHT)
    }

    /// Height of the area under the waveform: enough for the tallest label in view.
    fn label_lane(&self, ui: &egui::Ui, width: f32, view: WaveView) -> f32 {
        let axis = TimeAxis::new(egui::Rangef::new(0.0, width), view.start, view.end());
        let tallest = self
            .markers
            .iter()
            .filter(|m| (0.0..=width).contains(&axis.x(m.time)))
            .map(|m| Self::label_height(ui, m, self.state.selected == Some(m.id)))
            .fold(LABEL_MIN_HEIGHT, f32::max);
        tallest + LABEL_GAP + 4.0
    }

    fn handle_input(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        resp: &egui::Response,
        axis: TimeAxis,
    ) {
        let pointer_x = resp.interact_pointer_pos().map(|p| p.x);
        if resp.drag_started() {
            self.state.gesture = pointer_x
                .and_then(|x| marker_near(self.markers, axis, x))
                .map(Gesture::Marker);
            if let Some(Gesture::Marker(id)) = self.state.gesture {
                self.state.selected = Some(id);
            }
        }
        if resp.dragged() {
            let delta = resp.drag_delta();
            let x = pointer_x.unwrap_or(rect.center().x);
            if self.state.gesture.is_none() {
                // Where the drag began, not where the pointer has got to by now.
                let at = axis.t(x - delta.x);
                self.state.gesture = Some(Gesture::Navigate { at });
            }
            match self.state.gesture {
                Some(Gesture::Marker(id)) => {
                    if let Some(m) = self.markers.iter_mut().find(|m| m.id == id) {
                        m.time = (self.snap)(axis.t(x));
                    }
                }
                Some(Gesture::Navigate { at }) => {
                    let fraction = ((x - rect.left()) / rect.width().max(1.0)) as f64;
                    self.state.view.navigate(at, fraction, drag_zoom(delta.y));
                }
                None => {}
            }
        }
        if resp.drag_stopped() {
            self.state.gesture = None;
        }
        if resp.clicked() {
            self.state.selected = pointer_x.and_then(|x| marker_near(self.markers, axis, x));
        }
        if resp.double_clicked()
            && self.add_remove
            && let Some(x) = pointer_x
            && marker_near(self.markers, axis, x).is_none()
        {
            let m = (self.new_marker)((self.snap)(axis.t(x)));
            self.state.selected = Some(m.id);
            self.markers.push(m);
        }

        if resp.hovered() {
            let (scroll_x, zoom) = ui.input(|i| (i.smooth_scroll_delta().x, i.zoom_delta()));
            if scroll_x != 0.0 {
                self.state.view.pan(-scroll_x as f64 / axis.pps());
            }
            if zoom != 1.0 {
                let at = ui
                    .input(|i| i.pointer.hover_pos())
                    .map_or(self.state.view.start, |p| axis.t(p.x));
                self.state.view.zoom_at(at, 1.0 / zoom as f64);
            }
        }

        let typing = ui.ctx().egui_wants_keyboard_input();
        if self.add_remove
            && !typing
            && let Some(id) = self.state.selected
            && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
        {
            self.remove(id);
        }
    }

    fn remove(&mut self, id: u64) {
        self.markers.retain(|m| m.id != id);
        self.state.selected = None;
    }

    /// The text boxes under the waveform, the selected one editable. They grow with their
    /// text, which wraps.
    fn labels(&mut self, ui: &mut egui::Ui, rect: Rect, wave: Rect, axis: TimeAxis) {
        let top = wave.bottom() + LABEL_GAP;
        let selected = self.state.selected;
        let mut remove = None;
        for m in self.markers.iter_mut() {
            let x = axis.x(m.time);
            if !rect.x_range().contains(x) {
                continue;
            }
            let is_selected = selected == Some(m.id);
            let left = (x - LABEL_WIDTH / 2.0)
                .clamp(rect.left(), (rect.right() - LABEL_WIDTH).max(rect.left()));
            let height = Self::label_height(ui, m, is_selected);
            let bubble = Rect::from_min_size(pos2(left, top), vec2(LABEL_WIDTH, height));
            ui.painter().rect(
                bubble,
                CornerRadius::same(6),
                theme::panel(),
                Stroke::new(1.5, m.color),
                StrokeKind::Inside,
            );
            let text = bubble.shrink(LABEL_PADDING);
            if is_selected {
                let edit = text.with_max_x(text.right() - LABEL_BUTTON);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(edit));
                child.add(
                    egui::TextEdit::multiline(&mut m.label)
                        .frame(egui::Frame::NONE)
                        .font(label_font())
                        .text_color(m.color)
                        .desired_width(edit.width())
                        .desired_rows(1)
                        .hint_text("Note"),
                );
                if self.add_remove {
                    let spot = Rect::from_min_size(
                        pos2(text.right() - LABEL_BUTTON + 4.0, text.top()),
                        vec2(LABEL_BUTTON - 4.0, 24.0),
                    );
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(spot));
                    if child
                        .add(button(crate::icons::TRASH).small())
                        .on_hover_text("Delete marker")
                        .clicked()
                    {
                        remove = Some(m.id);
                    }
                }
            } else {
                let p = ui.painter();
                let galley = p.layout(m.label.clone(), label_font(), m.color, text.width());
                p.galley(text.left_top(), galley, m.color);
                let id = ui.id().with(("marker_label", m.id));
                if ui.interact(bubble, id, Sense::click()).clicked() {
                    self.state.selected = Some(m.id);
                }
            }
        }
        if let Some(id) = remove {
            self.remove(id);
        }
    }
}

/// Paints a marker over `wave`. Dotted lines reach up to the top of `rect`, so they can
/// run through whatever sits above the waveform.
pub fn paint_marker(p: &egui::Painter, axis: TimeAxis, m: &Marker, rect: Rect, wave: Rect) {
    let x = axis.x(m.time);
    match m.line {
        Line::Solid => {
            p.vline(x, wave.y_range(), Stroke::new(2.5, m.color));
        }
        Line::Dotted => dotted_vline(p, x, rect.top()..=wave.bottom(), m.color),
    }
    if let Some(end) = m.end {
        let x1 = axis.x(end);
        p.vline(x1, wave.y_range(), Stroke::new(1.0, m.color));
        let bar = Rect::from_x_y_ranges(x..=x1, (wave.top() - 8.0)..=wave.top());
        p.rect_filled(bar, 1.0, m.color.gamma_multiply(0.75));
    }
    if m.line == Line::Solid && m.end.is_none() {
        // A stem down to the label.
        p.vline(
            x,
            wave.bottom()..=wave.bottom() + 6.0,
            Stroke::new(2.5, m.color),
        );
    }
    if !m.label.is_empty() && m.line == Line::Dotted {
        p.text(
            pos2(x + 3.0, rect.top()),
            Align2::LEFT_TOP,
            &m.label,
            mono(10.0),
            m.color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Rangef;

    fn view() -> WaveView {
        WaveView::new(10.0, 20.0, (0.0, 100.0))
    }

    fn marker(id: u64, time: f64) -> Marker {
        Marker {
            id,
            time,
            end: None,
            label: String::new(),
            color: Color32::WHITE,
            line: Line::Solid,
        }
    }

    #[test]
    fn zoom_keeps_the_gesture_centre_in_place() {
        let mut v = view();
        v.zoom_at(15.0, 0.5);
        assert_eq!((v.start, v.span), (12.5, 5.0));
        let mut v = view();
        v.zoom_at(10.0, 0.5);
        assert_eq!(v.start, 10.0);
    }

    #[test]
    fn zoom_stops_at_the_limits() {
        let mut v = view();
        v.zoom_at(15.0, 1e-9);
        assert_eq!(v.span, MIN_SPAN);
        v.zoom_at(15.0, 1e12);
        assert_eq!((v.start, v.span), (0.0, 100.0));
    }

    #[test]
    fn pan_stays_in_bounds() {
        let mut v = view();
        v.pan(5.0);
        assert_eq!(v.start, 15.0);
        v.pan(-1000.0);
        assert_eq!(v.start, 0.0);
        v.pan(1000.0);
        assert_eq!(v.end(), 100.0);
    }

    #[test]
    fn view_of_a_short_clip_fits_inside_it() {
        let v = WaveView::new(0.0, 50.0, (0.0, 2.0));
        assert_eq!((v.start, v.span), (0.0, 2.0));
    }

    #[test]
    fn nearest_marker_within_reach_wins() {
        let axis = TimeAxis::new(Rangef::new(0.0, 1000.0), 0.0, 10.0);
        let markers = [marker(1, 1.0), marker(2, 1.05)];
        assert_eq!(marker_near(&markers, axis, 104.0), Some(2));
        assert_eq!(marker_near(&markers, axis, 99.0), Some(1));
        assert_eq!(marker_near(&markers, axis, 200.0), None);
    }

    #[test]
    fn dragging_down_zooms_in() {
        assert!(drag_zoom(10.0) < 1.0);
        assert!(drag_zoom(-10.0) > 1.0);
        assert_eq!(drag_zoom(0.0), 1.0);
    }

    #[test]
    fn one_drag_zooms_and_moves_at_once() {
        let mut v = view();
        // Holding time 15 (the middle), moved to a quarter of the width, zooming in.
        v.navigate(15.0, 0.25, 0.5);
        assert_eq!(v.span, 5.0);
        assert_eq!(v.start + 0.25 * v.span, 15.0);
    }

    #[test]
    fn the_held_point_follows_the_pointer_over_many_steps() {
        let mut v = view();
        for fraction in [0.5, 0.45, 0.4, 0.35] {
            v.navigate(15.0, fraction, drag_zoom(4.0));
            assert!((v.start + fraction * v.span - 15.0).abs() < 1e-9);
        }
        assert!(v.span < 10.0);
    }
}
