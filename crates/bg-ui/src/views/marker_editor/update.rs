//! How the editor's state changes. Pure, and needs no props, so an owner that routes
//! the editor's messages (a program) can call it from its own update.

use super::state::EditorState;
use super::{Emission, Msg};
use crate::kit::Emit;

pub fn update(s: &mut EditorState, msg: Msg, out: &mut Emit<Emission>) {
    match msg {
        Msg::Gesture(g) => s.gesture = g,
        Msg::Select(id) => s.selected = id,
        Msg::Navigate {
            at,
            fraction,
            factor,
        } => s.view.navigate(at, fraction, factor),
        Msg::Pan(by) => s.view.pan(by),
        Msg::Zoom { at, factor } => s.view.zoom_at(at, factor),
        Msg::MoveMarker { id, time } => out.send(Emission::Move { id, time }),
        Msg::Add(m) => {
            // Straight into typing its label.
            s.selected = Some(m.id);
            s.focus_label = 2;
            out.send(Emission::Add(m));
        }
        Msg::Remove(id) => {
            s.selected = None;
            out.send(Emission::Remove(id));
        }
        Msg::Relabel { id, text } => out.send(Emission::Relabel { id, text }),
        Msg::Focused => s.focus_label = s.focus_label.saturating_sub(1),
        Msg::FocusLabel(id) => {
            s.selected = Some(id);
            s.focus_label = 2;
        }
    }
}

/// Applies the editor's emissions to the markers its owner keeps.
pub fn apply(markers: &mut Vec<super::Marker>, e: Emission) {
    match e {
        Emission::Add(m) => markers.push(m),
        Emission::Move { id, time } => {
            if let Some(m) = markers.iter_mut().find(|m| m.id == id) {
                m.time = time;
            }
        }
        Emission::Remove(id) => markers.retain(|m| m.id != id),
        Emission::Relabel { id, text } => {
            if let Some(m) = markers.iter_mut().find(|m| m.id == id) {
                m.label = text;
            }
        }
    }
}
