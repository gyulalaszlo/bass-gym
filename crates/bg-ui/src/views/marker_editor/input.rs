//! The editor's interaction: egui's input for this frame read into a plain [`Input`],
//! and [`interpret`], a pure function from that to messages.

use egui::{Key, Rect, Response, Ui};

use super::Msg;
use super::state::{EditorState, Gesture, Marker};
use crate::widgets::time_axis::TimeAxis;

/// What happened to the editor's area this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    /// Where the pointer interacts, and where the current press started.
    pub pointer_x: Option<f32>,
    pub pressed_x: Option<f32>,
    pub hover_x: Option<f32>,
    pub drag_started: bool,
    pub dragged: bool,
    pub drag_dy: f32,
    pub drag_stopped: bool,
    pub clicked: bool,
    pub double_clicked: bool,
    pub hovered: bool,
    pub scroll_x: f32,
    pub zoom: f32,
    /// Delete or Backspace, while no text field has the keyboard.
    pub delete: bool,
}

pub fn read(ui: &Ui, resp: &Response) -> Input {
    let typing = ui.ctx().egui_wants_keyboard_input();
    // `Response` methods lock the context themselves, so they stay outside `ui.input`.
    let (pressed_x, hover_x, scroll_x, zoom, delete) = ui.input(|i| {
        (
            // A drag is only reported once the pointer has moved, so what it grabbed is
            // what was under the press, not under the pointer now.
            i.pointer.press_origin().map(|p| p.x),
            i.pointer.hover_pos().map(|p| p.x),
            i.smooth_scroll_delta().x,
            i.zoom_delta(),
            !typing && (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)),
        )
    });
    Input {
        pointer_x: resp.interact_pointer_pos().map(|p| p.x),
        pressed_x,
        hover_x,
        drag_started: resp.drag_started(),
        dragged: resp.dragged(),
        drag_dy: resp.drag_delta().y,
        drag_stopped: resp.drag_stopped(),
        clicked: resp.clicked(),
        double_clicked: resp.double_clicked(),
        hovered: resp.hovered(),
        scroll_x,
        zoom,
        delete,
    }
}

/// The marker within `reach` of screen position `x`, nearest first.
pub fn marker_near(markers: &[Marker], axis: TimeAxis, x: f32, reach: f32) -> Option<u64> {
    markers
        .iter()
        .map(|m| (m.id, (axis.x(m.time) - x).abs()))
        .filter(|&(_, d)| d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Zoom factor of a vertical drag: dragging down zooms in.
pub fn drag_zoom(dy: f32) -> f64 {
    (-dy as f64 * 0.01).exp()
}

/// What the editor needs to know to interpret input.
pub struct Context<'a> {
    pub markers: &'a [Marker],
    /// The editor's area, and its time axis at the start of the frame.
    pub rect: Rect,
    pub axis: TimeAxis,
    pub reach: f32,
    pub add_remove: bool,
    pub snap: &'a dyn Fn(f64) -> f64,
    /// A new marker at a time.
    pub new_marker: &'a dyn Fn(f64) -> Marker,
}

/// The messages `input` means. Steps within a frame depend on each other (a drag can
/// start and move in one frame), so the gesture and selection are followed locally.
pub fn interpret(state: &EditorState, c: &Context, input: &Input) -> Vec<Msg> {
    let mut msgs = Vec::new();
    let near = |x: f32| marker_near(c.markers, c.axis, x, c.reach);
    let mut gesture = state.gesture;
    let mut selected = state.selected;
    if input.drag_started {
        gesture = input.pressed_x.and_then(near).map(Gesture::Marker);
        if let Some(Gesture::Marker(id)) = gesture {
            selected = Some(id);
        }
    }
    if input.dragged {
        let x = input.pointer_x.unwrap_or(c.rect.center().x);
        if gesture.is_none() {
            let at = c.axis.t(input.pressed_x.unwrap_or(x));
            gesture = Some(Gesture::Navigate { at });
        }
        match gesture {
            Some(Gesture::Marker(id)) => msgs.push(Msg::MoveMarker {
                id,
                time: (c.snap)(c.axis.t(x)),
            }),
            Some(Gesture::Navigate { at }) => {
                let fraction = ((x - c.rect.left()) / c.rect.width().max(1.0)) as f64;
                msgs.push(Msg::Navigate {
                    at,
                    fraction,
                    factor: drag_zoom(input.drag_dy),
                });
            }
            None => {}
        }
    }
    if input.drag_stopped {
        gesture = None;
    }
    if input.clicked {
        selected = input.pointer_x.and_then(near);
    }
    if input.double_clicked
        && c.add_remove
        && let Some(x) = input.pointer_x
        && near(x).is_none()
    {
        let m = (c.new_marker)((c.snap)(c.axis.t(x)));
        selected = Some(m.id);
        msgs.push(Msg::Add(m));
    }
    if gesture != state.gesture {
        msgs.push(Msg::Gesture(gesture));
    }
    if selected != state.selected {
        msgs.push(Msg::Select(selected));
    }
    if input.hovered {
        if input.scroll_x != 0.0 {
            msgs.push(Msg::Pan(-input.scroll_x as f64 / c.axis.pps()));
        }
        if input.zoom != 1.0 {
            let at = input.hover_x.map_or(state.view.start, |x| c.axis.t(x));
            msgs.push(Msg::Zoom {
                at,
                factor: 1.0 / input.zoom as f64,
            });
        }
    }
    if c.add_remove
        && input.delete
        && let Some(id) = selected
    {
        msgs.push(Msg::Remove(id));
    }
    msgs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::marker_editor::state::{Line, WaveView};
    use egui::{Color32, Rangef, pos2};

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
    fn nearest_marker_within_reach_wins() {
        let axis = TimeAxis::new(Rangef::new(0.0, 1000.0), 0.0, 10.0);
        let markers = [marker(1, 1.0), marker(2, 1.05)];
        assert_eq!(marker_near(&markers, axis, 104.0, 6.0), Some(2));
        assert_eq!(marker_near(&markers, axis, 99.0, 6.0), Some(1));
        assert_eq!(marker_near(&markers, axis, 200.0, 6.0), None);
    }

    #[test]
    fn dragging_down_zooms_in() {
        assert!(drag_zoom(10.0) < 1.0);
        assert!(drag_zoom(-10.0) > 1.0);
        assert_eq!(drag_zoom(0.0), 1.0);
    }

    #[test]
    fn a_drag_that_starts_and_moves_in_one_frame_grabs_the_marker_under_the_press() {
        let markers = [marker(7, 1.0)];
        let state = EditorState::new(WaveView::new(0.0, 10.0, (0.0, 10.0)));
        let c = Context {
            markers: &markers,
            rect: Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 100.0)),
            axis: TimeAxis::new(Rangef::new(0.0, 1000.0), 0.0, 10.0),
            reach: 6.0,
            add_remove: true,
            snap: &|t| t,
            new_marker: &|t| marker(99, t),
        };
        let input = Input {
            pressed_x: Some(101.0),
            pointer_x: Some(150.0),
            drag_started: true,
            dragged: true,
            ..Input::default()
        };
        let msgs = interpret(&state, &c, &input);
        assert!(matches!(msgs[0], Msg::MoveMarker { id: 7, .. }));
        assert!(msgs.contains(&Msg::Gesture(Some(Gesture::Marker(7)))));
        assert!(msgs.contains(&Msg::Select(Some(7))));
    }
}
