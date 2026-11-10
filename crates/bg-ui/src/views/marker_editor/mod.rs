//! A zoomable waveform with markers on it. It knows nothing about where the audio or
//! markers come from: the owner provides a [`PeakSource`], read-only overlays and the
//! editable [`Marker`]s, so the same editor serves session reports and loop points.
//!
//! Gestures: dragging holds on to the point it started on, moving it with the pointer
//! while vertical movement zooms around it (down zooms in), so a single drag can do both.
//! Pinch or Ctrl/Cmd + wheel zoom too, and scrolling sideways pans. Drag an editable
//! marker to move it, double-click to add one, click one to select it and edit its text.
//!
//! A tier-2 [`Component`] whose state is [`EditorState`]. The markers belong to the
//! owner: the editor emits [`Emission`]s for them ([`update::apply`] applies them to a
//! `Vec`). Its [`update`] needs no props, so an owner that can't mount it (a program,
//! whose views only read) shows it with [`view`] and routes its [`Msg`]s to [`update`].
//!
//! - `state.rs`: markers, the visible range and the editor's state.
//! - `input.rs`: egui's input read into a plain struct, and interpreted (pure, tested).
//! - `update.rs`, `layout.rs`, `paint.rs`, `labels.rs` (the text boxes), `theme.rs`.

mod input;
mod labels;
mod layout;
mod paint;
mod state;
mod theme;
mod update;

use egui::{Rect, Sense, Ui, vec2};

use crate::kit::{Component, Emit};
use crate::widgets::time_axis::TimeAxis;
use crate::widgets::waveform::PeakSource;

pub use paint::paint_marker;
pub use state::{EditorState, Gesture, Line, MIN_SPAN, Marker, WaveView};
pub use theme::{LabelTheme, MarkerTheme, Metrics, Theme};
pub use update::{apply, update};

pub struct MarkerEditor;

/// Something drawn above the waveform on the same time axis, such as a score.
pub struct Header<'a> {
    pub height: f32,
    pub paint: &'a dyn Fn(&egui::Painter, Rect, TimeAxis),
}

pub struct Props<'a> {
    pub header: Option<Header<'a>>,
    pub source: &'a dyn PeakSource,
    /// Drawn but not editable.
    pub overlays: &'a [Marker],
    pub markers: &'a [Marker],
    /// Adjusts where a marker lands when placed or moved.
    pub snap: &'a dyn Fn(f64) -> f64,
    /// Double-click adds markers, and they can be deleted. Off when the owner has a fixed
    /// set, such as loop points.
    pub add_remove: bool,
    /// A new marker at a time (the owner picks its id and look).
    pub new_marker: &'a dyn Fn(f64) -> Marker,
    pub playhead: Option<f64>,
    /// Height of the waveform and its span bars; the header above and the labels below
    /// add to it.
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Gesture(Option<Gesture>),
    Select(Option<u64>),
    /// Drag the view by the point at time `at`: put it at `fraction` of the width and
    /// zoom by `factor`.
    Navigate {
        at: f64,
        fraction: f64,
        factor: f64,
    },
    Pan(f64),
    Zoom {
        at: f64,
        factor: f64,
    },
    MoveMarker {
        id: u64,
        time: f64,
    },
    Add(Marker),
    Remove(u64),
    Relabel {
        id: u64,
        text: String,
    },
    /// The selected label asked for the keyboard focus once more.
    Focused,
    /// A label was clicked: select its marker and edit its text.
    FocusLabel(u64),
}

/// Changes to the owner's markers.
#[derive(Clone, Debug, PartialEq)]
pub enum Emission {
    Add(Marker),
    Move { id: u64, time: f64 },
    Remove(u64),
    Relabel { id: u64, text: String },
}

/// Shows the editor and returns its messages, for an owner that routes them to
/// [`update`] itself (see the module docs). [`crate::kit::mount`] does both.
pub fn view(state: &EditorState, p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    let width = ui.available_width();
    let lane = layout::label_lane(ui, theme, p, state, width);
    let header = p.header.as_ref().map_or(0.0, |h| h.height);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(width, header + p.height + lane),
        Sense::click_and_drag(),
    );
    let l = layout::layout(rect, header, lane, theme.metrics.span_lane, state.view);
    let context = input::Context {
        markers: p.markers,
        rect,
        axis: l.axis,
        reach: theme.metrics.grab_distance,
        add_remove: p.add_remove,
        snap: p.snap,
        new_marker: p.new_marker,
    };
    for m in input::interpret(state, &context, &input::read(ui, &resp)) {
        out.send(m);
    }
    paint::paint(&ui.painter_at(rect), p, state, theme, &l);
    labels::labels(ui, p, state, theme, &l, out);
}

impl Component for MarkerEditor {
    type State = EditorState;
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Msg = Msg;
    type Emission = Emission;

    fn view(state: &EditorState, p: &Props, theme: &Theme, ui: &mut Ui, msg: &mut Emit<Msg>) {
        view(state, p, theme, ui, msg);
    }

    fn update(state: &mut EditorState, _p: &Props, msg: Msg, out: &mut Emit<Emission>) {
        update(state, msg, out);
    }
}

#[cfg(test)]
mod tests;
