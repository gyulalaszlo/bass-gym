//! Step editor for grooves.

use bg_core::{Groove, TimeSignature};
use egui::{Pos2, Rect, Sense, Stroke, StrokeKind, vec2};

use crate::theme;

/// Returns true when the groove changed.
pub fn groove_editor(ui: &mut egui::Ui, groove: &mut Groove, sig: TimeSignature) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        theme::caption(ui, "Grid");
        let mut spb = groove.steps_per_beat;
        egui::ComboBox::from_id_salt("groove_spb")
            .selected_text(spb_label(spb))
            .width(90.0)
            .show_ui(ui, |ui| {
                for v in [1, 2, 3, 4, 6, 8] {
                    ui.selectable_value(&mut spb, v, spb_label(v));
                }
            });
        if spb != groove.steps_per_beat {
            groove.set_steps_per_beat(spb);
            changed = true;
        }
        theme::caption(ui, "Bars");
        let mut bars = groove.bars;
        if ui.add(egui::DragValue::new(&mut bars).range(1..=8)).changed() {
            groove.bars = bars;
            let total = groove.total_steps(sig);
            groove.notes.retain(|n| n.step < total);
            changed = true;
        }
        if ui.button(format!("{} Clear", crate::icons::ERASER)).clicked() {
            groove.notes.clear();
            changed = true;
        }
    });

    let total = groove.total_steps(sig);
    let spb = groove.steps_per_beat.max(1);
    let per_bar = groove.steps_per_bar(sig);
    let cell = ((ui.available_width() - 4.0) / total as f32).clamp(4.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(cell * total as f32, 38.0), Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme::bg());
    for step in 0..total {
        let r = Rect::from_min_size(Pos2::new(rect.left() + step as f32 * cell, rect.top()), vec2(cell, rect.height())).shrink(1.0);
        match groove.note_at(step).map(|i| groove.notes[i]) {
            Some(n) if n.step == step => {
                p.rect_filled(r, 1.0, if n.accent { theme::text() } else { theme::cyan() });
            }
            Some(n) => {
                p.rect_filled(r.shrink2(vec2(0.0, 8.0)), 0.0, if n.accent { theme::text().gamma_multiply(0.5) } else { theme::cyan().gamma_multiply(0.45) });
            }
            None => {
                p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::grid()), StrokeKind::Inside);
            }
        }
        if step % spb == 0 {
            let (w, c) = if step % per_bar == 0 { (2.0, theme::line()) } else { (1.0, theme::dim()) };
            p.vline(r.left() - 1.0, rect.y_range(), Stroke::new(w, c));
        }
    }
    let step_at = |pos: Pos2| (((pos.x - rect.left()) / cell).floor() as u32).min(total - 1);
    if let Some(pos) = resp.interact_pointer_pos() {
        let step = step_at(pos);
        if resp.clicked() {
            if ui.input(|i| i.modifiers.shift) {
                groove.extend_to(step);
            } else {
                groove.toggle_step(step);
            }
            changed = true;
        } else if resp.secondary_clicked() {
            groove.toggle_accent(step);
            changed = true;
        }
    }
    resp.on_hover_text("Click: add / remove note · Shift+click: extend previous note · Right-click: accent");
    if changed {
        groove.name = custom_name(&groove.name);
    }
    changed
}

fn spb_label(v: u32) -> String {
    match v {
        1 => "1/4".into(),
        2 => "1/8".into(),
        3 => "1/8 triplet".into(),
        4 => "1/16".into(),
        6 => "1/16 triplet".into(),
        8 => "1/32".into(),
        _ => format!("{v} per beat"),
    }
}

fn custom_name(name: &str) -> String {
    if name.ends_with("(edited)") { name.to_owned() } else { format!("{name} (edited)") }
}

