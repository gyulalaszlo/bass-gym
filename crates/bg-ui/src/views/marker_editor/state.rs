//! The editor's data: markers, the visible range and what it remembers between frames.

use egui::Color32;

/// Smallest visible span, in axis units.
pub const MIN_SPAN: f64 = 0.005;

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
pub enum Gesture {
    Marker(u64),
    /// Dragging the view by the point at time `at`.
    Navigate {
        at: f64,
    },
}

/// What the editor remembers between frames. Owned by whoever shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorState {
    pub view: WaveView,
    pub selected: Option<u64>,
    /// Show levels on a log scale.
    pub log: bool,
    pub(crate) gesture: Option<Gesture>,
    /// Frames left to ask for keyboard focus on the selected label. More than one, as the
    /// click that opened it can take the focus away again on the same frame.
    pub(crate) focus_label: u8,
}

/// An editor over 0..1, for an owner that sets the view before showing it.
impl Default for EditorState {
    fn default() -> Self {
        Self::new(WaveView::new(0.0, 1.0, (0.0, 1.0)))
    }
}

impl EditorState {
    pub fn new(view: WaveView) -> Self {
        Self {
            view,
            selected: None,
            log: false,
            gesture: None,
            focus_label: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> WaveView {
        WaveView::new(10.0, 20.0, (0.0, 100.0))
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
}
