//! Interaction tests: the editor in a real egui context, its messages applied each
//! frame as an owner would (view, then update, then apply the emissions).

use egui::{Color32, Key, Pos2, Rangef, pos2, vec2};

use super::*;
use crate::kit::Rig;
use crate::theme::Tokens;

struct Flat;

impl PeakSource for Flat {
    fn range(&self) -> (f64, f64) {
        (0.0, 100.0)
    }

    fn peak(&self, _from: f64, _to: f64) -> Option<(f32, f32)> {
        Some((-0.5, 0.5))
    }
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

/// The editor and the markers it edits, as their owner keeps them.
struct Harness {
    rig: Rig,
    state: EditorState,
    markers: Vec<Marker>,
    theme: Theme,
}

impl Harness {
    const AT: Pos2 = pos2(300.0, 120.0);

    fn new() -> Self {
        Self {
            rig: Rig::new(),
            state: EditorState::new(WaveView::new(10.0, 20.0, (0.0, 100.0))),
            markers: Vec::new(),
            theme: Theme::new(&Tokens::default()),
        }
    }

    fn run(&mut self, f: impl FnOnce(&mut Rig, &mut dyn FnMut(&mut egui::Ui))) {
        let (state, markers, theme) = (&mut self.state, &mut self.markers, &self.theme);
        let mut ui_fn = |ui: &mut egui::Ui| {
            let next = markers.iter().map(|m| m.id).max().unwrap_or(0) + 1;
            let new_marker = |t| marker(next, t);
            let props = Props {
                header: None,
                source: &Flat,
                overlays: &[],
                markers,
                snap: &|t| t,
                add_remove: true,
                new_marker: &new_marker,
                playhead: None,
                height: 200.0,
            };
            let mut msgs = Emit::new();
            view(state, &props, theme, ui, &mut msgs);
            let mut out = Emit::new();
            for m in msgs {
                update(state, m, &mut out);
            }
            for e in out {
                apply(markers, e);
            }
        };
        f(&mut self.rig, &mut ui_fn);
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.run(|rig, ui| rig.frame(events, ui));
    }

    fn double_click(&mut self, at: Pos2) {
        self.run(|rig, ui| rig.double_click(at, ui));
    }

    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.run(|rig, ui| rig.drag(from, to, ui));
    }

    fn type_text(&mut self, text: &str) {
        self.run(|rig, ui| rig.type_text(text, ui));
    }
}

#[test]
fn typing_goes_straight_into_a_new_marker() {
    let mut h = Harness::new();
    h.double_click(Harness::AT);
    assert_eq!(h.markers.len(), 1);
    h.frame(Vec::new());
    h.type_text("late");
    h.type_text(" note");
    assert_eq!(h.markers[0].label, "late note");
}

#[test]
fn backspace_while_typing_edits_the_text_not_the_marker() {
    let mut h = Harness::new();
    h.double_click(Harness::AT);
    h.frame(Vec::new());
    h.type_text("ab");
    h.frame(vec![egui::Event::Key {
        key: Key::Backspace,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert_eq!(h.markers.len(), 1);
    assert_eq!(h.markers[0].label, "a");
}

#[test]
fn dragging_down_zooms_in_and_up_zooms_out() {
    let mut h = Harness::new();
    h.drag(Harness::AT, Harness::AT + vec2(0.0, 60.0));
    let zoomed_in = h.state.view.span;
    assert!(zoomed_in < 10.0);
    h.drag(Harness::AT, Harness::AT - vec2(0.0, 60.0));
    assert!(h.state.view.span > zoomed_in);
}

#[test]
fn one_drag_moves_and_zooms_together() {
    let mut h = Harness::new();
    let held = TimeAxis::new(Rangef::new(0.0, 800.0), 10.0, 20.0).t(Harness::AT.x);
    let to = Harness::AT + vec2(100.0, 60.0);
    h.drag(Harness::AT, to);
    let v = h.state.view;
    assert!(v.span < 10.0);
    let now = TimeAxis::new(Rangef::new(0.0, 800.0), v.start, v.end()).t(to.x);
    assert!((now - held).abs() < 0.2, "{now} vs {held}");
}

#[test]
fn dragging_a_marker_moves_it_not_the_view() {
    let mut h = Harness::new();
    h.markers.push(marker(1, 15.0));
    h.frame(Vec::new());
    let before = h.state.view;
    h.drag(pos2(400.0, 120.0), pos2(480.0, 120.0));
    assert_eq!(h.state.view, before);
    assert!((h.markers[0].time - 16.0).abs() < 0.05);
}

#[test]
fn an_idle_editor_sends_nothing() {
    let mut h = Harness::new();
    h.markers.push(marker(1, 15.0));
    h.frame(Vec::new());
    let (state, markers) = (h.state.clone(), h.markers.clone());
    h.frame(Vec::new());
    assert_eq!((h.state, h.markers), (state, markers));
}
