//! The text boxes under the waveform: the selected one editable (on a copy of its
//! text), the others clickable. They grow with their text, which wraps.

use egui::{CornerRadius, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::layout::{Layout, label_height};
use super::state::EditorState;
use super::theme::Theme;
use super::{Msg, Props};
use crate::kit::Emit;
use crate::widgets::button::button;

pub fn labels(
    ui: &mut Ui,
    p: &Props,
    state: &EditorState,
    theme: &Theme,
    l: &Layout,
    out: &mut Emit<Msg>,
) {
    let t = &theme.label;
    let (rect, top) = (l.rect, l.wave.bottom() + theme.metrics.label_gap);
    for m in p.markers {
        let x = l.axis.x(m.time);
        if !rect.x_range().contains(x) {
            continue;
        }
        let selected = state.selected == Some(m.id);
        let left =
            (x - t.width / 2.0).clamp(rect.left(), (rect.right() - t.width).max(rect.left()));
        let height = label_height(ui, theme, m, selected);
        let bubble = Rect::from_min_size(pos2(left, top), vec2(t.width, height));
        ui.painter().rect(
            bubble,
            CornerRadius::same(t.rounding),
            t.fill,
            Stroke::new(t.stroke_width, m.color),
            StrokeKind::Inside,
        );
        let text = bubble.shrink(t.padding);
        if selected {
            let edit = text.with_max_x(text.right() - t.button_width);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(edit));
            let mut label = m.label.clone();
            let field = child.add(
                egui::TextEdit::multiline(&mut label)
                    .frame(egui::Frame::NONE)
                    .font(t.font.clone())
                    .text_color(m.color)
                    .desired_width(edit.width())
                    .desired_rows(1)
                    .hint_text("Note"),
            );
            if label != m.label {
                out.send(Msg::Relabel {
                    id: m.id,
                    text: label,
                });
            }
            if state.focus_label > 0 {
                field.request_focus();
                out.send(Msg::Focused);
            }
            if p.add_remove {
                let spot = Rect::from_min_size(
                    pos2(text.right() - t.button_width + 4.0, text.top()),
                    vec2(t.button_width - 4.0, t.button_height),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(spot));
                if child
                    .add(button(crate::icons::TRASH).small())
                    .on_hover_text("Delete marker")
                    .clicked()
                {
                    out.send(Msg::Remove(m.id));
                }
            }
        } else {
            let painter = ui.painter();
            let galley = painter.layout(m.label.clone(), t.font.clone(), m.color, text.width());
            painter.galley(text.left_top(), galley, m.color);
            let id = ui.id().with(("marker_label", m.id));
            if ui.interact(bubble, id, Sense::click()).clicked() {
                out.send(Msg::FocusLabel(m.id));
            }
        }
    }
}
