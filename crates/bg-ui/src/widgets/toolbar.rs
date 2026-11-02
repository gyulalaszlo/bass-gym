//! The Exercise Control Toolbar: a fixed-height strip above the exercise area that stays
//! visible while the exercise scrolls. Exercises fill it through
//! [`crate::ExerciseType::toolbar`] using the building blocks here, so every exercise
//! shares the same look: start / stop, tempo, time signature, option menus and a live
//! status. Items are separated by [`Gap`]s that share out the free width, so the
//! toolbar always spans the full width with a balanced layout.

use std::cell::Cell;

use bg_core::TimeSignature;
use egui::{Align, Layout, Pos2, Rect, RichText, Shape, Ui, vec2};

use crate::widgets::button::{button, selectable};
use crate::widgets::controls::{self, number_field};
use crate::{icons, theme};

/// Height of the toolbar row.
pub const TOOLBAR_HEIGHT: f32 = 48.0;

/// Horizontal padding around a menu button's icon (and its tab when open).
const MENU_PADDING: f32 = 12.0;

/// Height of the buttons in the toolbar.
pub const BUTTON_HEIGHT: f32 = 36.0;

/// Space between toolbar items. Each gap has a minimum width and takes a share of the
/// free width in proportion to its weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    Small,
    Medium,
    Large,
}

impl Gap {
    fn min(self) -> f32 {
        match self {
            Gap::Small => 8.0,
            Gap::Medium => 16.0,
            Gap::Large => 24.0,
        }
    }

    fn weight(self) -> f32 {
        match self {
            Gap::Small => 1.0,
            Gap::Medium => 2.0,
            Gap::Large => 6.0,
        }
    }
}

/// Gap bookkeeping for one frame.
#[derive(Default)]
struct GapTally {
    /// Width taken by gaps so far.
    width: Cell<f32>,
    min: Cell<f32>,
    weight: Cell<f32>,
}

/// Layout measured on the previous frame: items' width, sum of gap minimums and weights.
#[derive(Clone, Copy, Default)]
struct Measured {
    items: f32,
    min: f32,
    weight: f32,
}

pub struct Toolbar<'a> {
    ui: &'a mut Ui,
    tally: &'a GapTally,
    /// Free width to share between the gaps (from the previous frame).
    extra: f32,
    weights: f32,
}

/// Shows the toolbar across the full width.
pub fn show_toolbar<R>(ui: &mut Ui, add: impl FnOnce(&mut Toolbar) -> R) -> R {
    let width = ui.available_width();
    let id = ui.id().with("exercise_toolbar");
    let last: Measured = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let tally = GapTally::default();
    let (r, now) = ui
        .allocate_ui_with_layout(vec2(width, TOOLBAR_HEIGHT), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_size(vec2(width, TOOLBAR_HEIGHT));
            ui.spacing_mut().item_spacing.x = 0.0;
            let left = ui.cursor().left();
            let extra = (width - last.items - last.min).max(0.0);
            let r = add(&mut Toolbar { ui, tally: &tally, extra, weights: last.weight });
            let used = ui.cursor().left() - left;
            (r, Measured { items: used - tally.width.get(), min: tally.min.get(), weight: tally.weight.get() })
        })
        .inner;
    if (now.items - last.items).abs() > 0.5 || now.weight != last.weight || now.min != last.min {
        ui.data_mut(|d| d.insert_temp(id, now));
        ui.ctx().request_repaint();
    }
    r
}

impl Toolbar<'_> {
    fn gap_width(&self, gap: Gap) -> f32 {
        let share = if self.weights > 0.0 { self.extra * gap.weight() / self.weights } else { 0.0 };
        gap.min() + share
    }

    fn tally(&self, gap: Gap, w: f32) {
        let t = self.tally;
        t.width.set(t.width.get() + w);
        t.min.set(t.min.get() + gap.min());
        t.weight.set(t.weight.get() + gap.weight());
    }

    /// Space between two items.
    pub fn gap(&mut self, gap: Gap) {
        let w = self.gap_width(gap);
        self.ui.add_space(w);
        self.tally(gap, w);
    }

    /// A gap that shows something in its space (left-aligned, clipped to the gap), e.g.
    /// the live status. Its contents don't change the layout.
    pub fn gap_with<R>(&mut self, gap: Gap, add: impl FnOnce(&mut Ui) -> R) -> R {
        let w = self.gap_width(gap);
        let (rect, _) = self.ui.allocate_exact_size(vec2(w, TOOLBAR_HEIGHT), egui::Sense::hover());
        self.tally(gap, w);
        let inner = rect.shrink2(vec2(Gap::Medium.min(), 0.0));
        let mut child = self.ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
        child.set_clip_rect(inner.intersect(self.ui.clip_rect()));
        child.spacing_mut().item_spacing.x = 10.0;
        add(&mut child)
    }

    /// The underlying row, for exercise-specific controls.
    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    /// A group of items, enabled or not (e.g. settings are locked while a session runs).
    pub fn group<R>(&mut self, enabled: bool, add: impl FnOnce(&mut Toolbar) -> R) -> R {
        let (tally, extra, weights) = (self.tally, self.extra, self.weights);
        self.ui.add_enabled_ui(enabled, |ui| add(&mut Toolbar { ui, tally, extra, weights })).inner
    }

    /// The boxed start / stop button. Also answers the Space key. Returns true when the
    /// session should start or stop.
    pub fn transport(&mut self, running: bool) -> bool {
        let ui = &mut *self.ui;
        let space = ui.input(|i| i.key_pressed(egui::Key::Space)) && !ui.ctx().egui_wants_keyboard_input();
        let (text, color) = if running { (format!("{}  STOP", icons::STOP), theme::amber()) } else { (format!("{}  START", icons::PLAY), theme::green()) };
        let clicked = ui
            .add(button(text).color(color).strong().font(egui::FontId::proportional(16.0)).min_size([118.0, BUTTON_HEIGHT]))
            .on_hover_text("Space: start / stop")
            .clicked();
        clicked || space
    }

    /// The boxed tap-tempo button. Sets `bpm` once tapped at least 3 times.
    pub fn tap(&mut self, bpm: &mut f32) -> bool {
        let ui = &mut *self.ui;
        let r = ui.add(button(format!("{} TAP", icons::HAND_TAP)).min_size([88.0, BUTTON_HEIGHT])).on_hover_text("Tap at least 3 times in tempo");
        if r.clicked()
            && let Some(t) = tap_tempo(ui)
        {
            *bpm = t.round().clamp(20.0, 400.0);
            return true;
        }
        false
    }

    /// − [large BPM readout] bpm +. The field steps by 5 with ↑ / ↓, the buttons by 1.
    pub fn bpm(&mut self, bpm: &mut f32) -> bool {
        let before = *bpm;
        self.ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let step = |ui: &mut Ui, icon: &str, tip: &str| ui.add(egui::Button::new(RichText::new(icon).size(18.0).color(theme::cyan())).frame(false)).on_hover_text(tip).clicked();
            if step(ui, icons::MINUS, "−1 BPM") {
                *bpm = (*bpm - 1.0).max(20.0);
            }
            ui.add(number_field(bpm).range(20.0..=400.0).step(5.0).decimals(0).font(theme::mono(28.0)).suffix("bpm").id_salt("toolbar_bpm"))
                .on_hover_text("Type a tempo, or use ↑ / ↓ (±5, Shift ±50)");
            if step(ui, icons::PLUS, "+1 BPM") {
                *bpm = (*bpm + 1.0).min(400.0);
            }
        });
        *bpm != before
    }

    /// The time signature, as a "4/4" dropdown with common signatures and custom fields.
    pub fn time_sig(&mut self, sig: &mut TimeSignature) -> bool {
        let before = *sig;
        let text = RichText::new(format!("{}/{}", sig.num, sig.den)).font(theme::mono(20.0));
        controls::dropdown(self.ui, "toolbar_time_sig", text, |ui| {
            for (n, d) in [(2, 4), (3, 4), (4, 4), (5, 4), (6, 8), (7, 8), (9, 8), (12, 8)] {
                if ui.add(selectable(sig.num == n && sig.den == d, format!("{n}/{d}"))).clicked() {
                    *sig = TimeSignature { num: n, den: d };
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(number_field(&mut sig.num).range(1..=15).id_salt("ts_num"));
                ui.label("/");
                controls::dropdown_value(ui, "ts_den", &mut sig.den, [2, 4, 8, 16], |d| d.to_string());
            });
        });
        *sig != before
    }

    /// An icon button with a caret that opens a panel of options below it. The open
    /// panel and the button form one shape: the panel with a tab on top. `active`
    /// highlights the icon (e.g. when any of the options is on).
    pub fn menu<R>(&mut self, id_salt: &str, icon: &str, tooltip: &str, active: bool, width: f32, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let ui = &mut *self.ui;
        let id = ui.make_persistent_id(id_salt);
        let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let color = if !ui.is_enabled() {
            theme::dim()
        } else if active {
            theme::green()
        } else {
            theme::cyan()
        };
        let label = format!("{icon} {}", icons::CARET_DOWN);
        let font = egui::FontId::proportional(20.0);
        // Drawn by hand so the label has a fixed horizontal margin (it becomes the tab).
        let galley = ui.painter().layout_no_wrap(label.clone(), font.clone(), color);
        let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 2.0 * MENU_PADDING, BUTTON_HEIGHT), egui::Sense::click());
        let resp = resp.on_hover_text(tooltip);
        if ui.is_rect_visible(rect) {
            let c = if resp.hovered() && ui.is_enabled() { theme::text() } else { color };
            ui.painter().galley(rect.center() - galley.size() / 2.0, galley, c);
        }
        if resp.clicked() {
            open = !open;
        }
        // While one of the panel's own dropdowns is open, a click is that dropdown's.
        let child_open = egui::Popup::is_any_open(ui.ctx());
        let behavior = if child_open { egui::PopupCloseBehavior::IgnoreClicks } else { egui::PopupCloseBehavior::CloseOnClickOutside };
        let tab = resp.rect.with_min_y(resp.rect.top() - 2.0).with_max_y(resp.rect.bottom() + 6.0);
        let inner = egui::Popup::from_response(&resp)
            .id(id.with("panel"))
            .anchor(tab)
            .open_bool(&mut open)
            .close_behavior(behavior)
            .align(egui::RectAlign::BOTTOM_END)
            .align_alternatives(&[egui::RectAlign::BOTTOM_START])
            .gap(0.0)
            .width(width)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::same(14)))
            .show(|ui| {
                // Placeholder for the sheet, painted under the contents once its size is known.
                let sheet = ui.ctx().layer_painter(ui.layer_id()).add(Shape::Noop);
                ui.set_width(width);
                (sheet, ui.is_sizing_pass(), add(ui))
            });
        ui.data_mut(|d| d.insert_temp(id, open));
        let inner = inner?;
        let (sheet, sizing, r) = inner.inner;
        if !sizing {
            let painter = ui.ctx().layer_painter(inner.response.layer_id);
            let (fill, stroke) = (theme::panel(), egui::Stroke::new(1.0, theme::line()));
            let p = inner.response.rect;
            let outline = sheet_outline(p, tab);
            painter.set(sheet, Shape::Vec(vec![Shape::rect_filled(p, 0.0, fill), Shape::rect_filled(tab, 0.0, fill), Shape::closed_line(outline, stroke)]));
            // The button is under the sheet; draw its label again on the tab.
            painter.text(resp.rect.center(), egui::Align2::CENTER_CENTER, label, font, color);
        }
        Some(r)
    }

    /// A thin vertical divider.
    pub fn separator(&mut self) {
        self.ui.separator();
    }

}

fn tap_tempo(ui: &Ui) -> Option<f32> {
    let id = egui::Id::new("tap_tempo");
    let now = ui.input(|i| i.time);
    let mut taps: Vec<f64> = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    if taps.last().is_some_and(|&t| now - t > 2.0) {
        taps.clear();
    }
    taps.push(now);
    if taps.len() > 8 {
        taps.remove(0);
    }
    ui.data_mut(|d| d.insert_temp(id, taps.clone()));
    (taps.len() >= 3).then(|| 60.0 * (taps.len() - 1) as f32 / (taps[taps.len() - 1] - taps[0]) as f32)
}

/// Outline of a panel with a tab on its top edge (the tab sits on the panel, flush with
/// one of its sides or anywhere in between).
fn sheet_outline(p: Rect, t: Rect) -> Vec<Pos2> {
    let pts = [
        p.left_top(),
        Pos2::new(t.left(), p.top()),
        t.left_top(),
        t.right_top(),
        Pos2::new(t.right(), p.top()),
        p.right_top(),
        p.right_bottom(),
        p.left_bottom(),
    ];
    let mut out: Vec<Pos2> = Vec::with_capacity(pts.len());
    for q in pts {
        if out.last().is_none_or(|l| l.distance(q) > 0.5) {
            out.push(q);
        }
    }
    out
}
