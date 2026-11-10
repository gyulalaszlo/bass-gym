//! [`field`] and [`edited`]: editing that sends a message instead of writing the value.

use egui::{Response, Ui};

use super::Emit;

/// Runs `widget` on a copy of `value` and, if the copy changed, sends `msg(new)`.
///
/// This keeps form code nearly as short as editing in place, while the view only reads
/// its data:
///
/// ```ignore
/// field(ui, &props.bpm, out, Msg::Bpm, |ui, v| {
///     ui.add(number_field(v).range(20.0..=400.0))
/// });
/// field(ui, &props.click, out, Msg::Click, |ui, v| checkbox(ui, v, "Click"));
/// ```
pub fn field<T, M>(
    ui: &mut Ui,
    value: &T,
    out: &mut Emit<M>,
    msg: impl FnOnce(T) -> M,
    widget: impl FnOnce(&mut Ui, &mut T) -> Response,
) -> Response
where
    T: Clone + PartialEq,
{
    let mut copy = value.clone();
    let resp = widget(ui, &mut copy);
    if copy != *value {
        out.send(msg(copy));
    }
    resp
}

/// Runs `edit` on a copy of `value` and returns the copy if it changed: for editing
/// code that isn't a single widget (a dropdown's list, a group of radio buttons, a
/// widget that edits in place):
///
/// ```ignore
/// if let Some(kind) = edited(&p.kind, |k| radio_group(ui, k)) {
///     out.send(Emission::Kind(kind));
/// }
/// ```
pub fn edited<T: Clone + PartialEq>(value: &T, edit: impl FnOnce(&mut T)) -> Option<T> {
    let mut copy = value.clone();
    edit(&mut copy);
    (copy != *value).then_some(copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::Rig;
    use crate::widgets::controls::checkbox;
    use std::cell::{Cell, RefCell};

    #[test]
    fn edited_returns_only_changes() {
        assert_eq!(edited(&1, |v| *v += 1), Some(2));
        assert_eq!(edited(&1, |v| *v *= 1), None);
    }

    #[test]
    fn sends_the_new_value_only_when_it_changes_and_leaves_the_value_alone() {
        let on = false;
        let sent = RefCell::new(Vec::new());
        let rect = Cell::new(egui::Rect::NOTHING);
        let mut ui_fn = |ui: &mut Ui| {
            let mut out = Emit::new();
            rect.set(field(ui, &on, &mut out, |v| v, |ui, v| checkbox(ui, v, "On")).rect);
            sent.borrow_mut().extend(out);
        };
        let mut rig = Rig::new();
        rig.frame(Vec::new(), &mut ui_fn);
        assert!(sent.borrow().is_empty());
        rig.click(rect.get().center(), &mut ui_fn);
        assert_eq!(*sent.borrow(), vec![true]);
        assert!(!on);
    }
}
