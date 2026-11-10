//! Step editor for grooves. A tier-1 view: it emits the edited groove.

use bg_core::{Groove, TimeSignature};
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};

use crate::kit::{Emit, View, edited};
use crate::theme::{Tokens, caption};

pub struct GrooveEditor;

pub struct Props<'a> {
    pub groove: &'a Groove,
    pub sig: TimeSignature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub background: Color32,
    /// A note's first step, accented and not; then the steps it holds over.
    pub accent: Color32,
    pub note: Color32,
    pub held_accent: Color32,
    pub held: Color32,
    pub empty: Stroke,
    pub bar_line: Stroke,
    pub beat_line: Stroke,
    pub height: f32,
    /// A step's width, within limits.
    pub step_limits: (f32, f32),
    /// How much shorter a held step is drawn, above and below.
    pub held_inset: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            background: t.bg(),
            accent: t.text(),
            note: t.cyan(),
            held_accent: t.text().gamma_multiply(0.5),
            held: t.cyan().gamma_multiply(0.45),
            empty: Stroke::new(1.0, t.grid()),
            bar_line: Stroke::new(2.0, t.line()),
            beat_line: Stroke::new(1.0, t.dim()),
            height: 38.0,
            step_limits: (4.0, 36.0),
            held_inset: 8.0,
        }
    }
}

impl View for GrooveEditor {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Groove;

    fn view(p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Groove>) {
        let sig = p.sig;
        let edit = edited(p.groove, |groove| {
            if editor(ui, theme, groove, sig) {
                groove.name = custom_name(&groove.name);
            }
        });
        if let Some(g) = edit {
            out.send(g);
        }
    }
}

/// Edits `groove` in place (on the view's copy). Returns true when it changed.
fn editor(ui: &mut Ui, theme: &Theme, groove: &mut Groove, sig: TimeSignature) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        caption(ui, "Grid");
        let mut spb = groove.steps_per_beat;
        crate::widgets::controls::dropdown_value(
            ui,
            "groove_spb",
            &mut spb,
            [1, 2, 3, 4, 6, 8],
            |v| spb_label(*v).to_string(),
        );
        if spb != groove.steps_per_beat {
            groove.set_steps_per_beat(spb);
            changed = true;
        }
        caption(ui, "Bars");
        let mut bars = groove.bars;
        if ui
            .add(crate::widgets::controls::number_field(&mut bars).range(1..=8))
            .changed()
        {
            groove.bars = bars;
            let total = groove.total_steps(sig);
            groove.notes.retain(|n| n.step < total);
            changed = true;
        }
        if ui
            .add(crate::widgets::button(format!(
                "{} Clear",
                crate::icons::ERASER
            )))
            .clicked()
        {
            groove.notes.clear();
            changed = true;
        }
    });

    let total = groove.total_steps(sig);
    let spb = groove.steps_per_beat.max(1);
    let per_bar = groove.steps_per_bar(sig);
    let (min, max) = theme.step_limits;
    let cell = ((ui.available_width() - 4.0) / total as f32).clamp(min, max);
    let size = vec2(cell * total as f32, theme.height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.background);
    for step in 0..total {
        let r = Rect::from_min_size(
            Pos2::new(rect.left() + step as f32 * cell, rect.top()),
            vec2(cell, rect.height()),
        )
        .shrink(1.0);
        match groove.note_at(step).map(|i| groove.notes[i]) {
            Some(n) if n.step == step => {
                p.rect_filled(r, 1.0, if n.accent { theme.accent } else { theme.note });
            }
            Some(n) => {
                let color = if n.accent {
                    theme.held_accent
                } else {
                    theme.held
                };
                p.rect_filled(r.shrink2(vec2(0.0, theme.held_inset)), 0.0, color);
            }
            None => {
                p.rect_stroke(r, 0.0, theme.empty, StrokeKind::Inside);
            }
        }
        if step % spb == 0 {
            let line = if step % per_bar == 0 {
                theme.bar_line
            } else {
                theme.beat_line
            };
            p.vline(r.left() - 1.0, rect.y_range(), line);
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
    resp.on_hover_text(
        "Click: add / remove note · Shift+click: extend previous note · Right-click: accent",
    );
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
    if name.ends_with("(edited)") {
        name.to_owned()
    } else {
        format!("{name} (edited)")
    }
}
