//! The Exercise Control Toolbar: a fixed-height strip above the exercise area that stays
//! visible while the exercise scrolls. Exercises fill it through
//! [`crate::ExerciseType::toolbar`] using the building blocks here, so every exercise
//! shares the same look: start / stop, tempo, time signature, option menus and a live
//! status. Items are separated by [`Gap`]s that share out the free width, so the
//! toolbar always spans the full width with a balanced layout.

use std::cell::Cell;

use bg_core::TimeSignature;
use bg_core::harmony::{Mode, Scale};
use bg_core::session::TempoRamp;
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

/// A chip for [`Toolbar::chips`].
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub icon: &'static str,
    /// A few words, e.g. "4+4 bars".
    pub label: String,
    /// The full description, shown on hover.
    pub tooltip: String,
}

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
        .allocate_ui_with_layout(
            vec2(width, TOOLBAR_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_size(vec2(width, TOOLBAR_HEIGHT));
                ui.spacing_mut().item_spacing.x = 0.0;
                let left = ui.cursor().left();
                let extra = (width - last.items - last.min).max(0.0);
                let r = add(&mut Toolbar {
                    ui,
                    tally: &tally,
                    extra,
                    weights: last.weight,
                });
                let used = ui.cursor().left() - left;
                (
                    r,
                    Measured {
                        items: used - tally.width.get(),
                        min: tally.min.get(),
                        weight: tally.weight.get(),
                    },
                )
            },
        )
        .inner;
    if (now.items - last.items).abs() > 0.5 || now.weight != last.weight || now.min != last.min {
        ui.data_mut(|d| d.insert_temp(id, now));
        ui.ctx().request_repaint();
    }
    r
}

impl Toolbar<'_> {
    fn gap_width(&self, gap: Gap) -> f32 {
        let share = if self.weights > 0.0 {
            self.extra * gap.weight() / self.weights
        } else {
            0.0
        };
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
        let (rect, _) = self
            .ui
            .allocate_exact_size(vec2(w, TOOLBAR_HEIGHT), egui::Sense::hover());
        self.tally(gap, w);
        let inner = rect.shrink2(vec2(Gap::Medium.min(), 0.0));
        let mut child = self.ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
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
        self.ui
            .add_enabled_ui(enabled, |ui| {
                add(&mut Toolbar {
                    ui,
                    tally,
                    extra,
                    weights,
                })
            })
            .inner
    }

    /// The boxed start / stop button. Also answers the Space key. Returns true when the
    /// session should start or stop.
    pub fn transport(&mut self, running: bool) -> bool {
        let ui = &mut *self.ui;
        let space = ui.input(|i| i.key_pressed(egui::Key::Space))
            && !ui.ctx().egui_wants_keyboard_input()
            && !crate::widgets::results::modal_open(ui.ctx());
        let (text, color) = if running {
            (format!("{}  STOP", icons::STOP), theme::amber())
        } else {
            (format!("{}  START", icons::PLAY), theme::green())
        };
        let clicked = ui
            .add(
                button(text)
                    .color(color)
                    .strong()
                    .font(egui::FontId::proportional(16.0))
                    .min_size([118.0, BUTTON_HEIGHT]),
            )
            .on_hover_text("Space: start / stop")
            .clicked();
        clicked || space
    }

    /// A small button that (re)opens the last session's results; disabled while there
    /// are none. Returns true when clicked.
    pub fn results_button(&mut self, available: bool) -> bool {
        let ui = &mut *self.ui;
        ui.add_enabled_ui(available, |ui| {
            ui.add(
                button(icons::CHART_BAR)
                    .font(egui::FontId::proportional(18.0))
                    .min_size([BUTTON_HEIGHT, BUTTON_HEIGHT]),
            )
            .on_hover_text("Results of the last session")
            .on_disabled_hover_text("No results yet: run a session")
            .clicked()
        })
        .inner
    }

    /// The boxed tap-tempo button. Sets `bpm` once tapped at least 3 times.
    pub fn tap(&mut self, bpm: &mut f32) -> bool {
        let ui = &mut *self.ui;
        let r = ui
            .add(button(format!("{} TAP", icons::HAND_TAP)).min_size([88.0, BUTTON_HEIGHT]))
            .on_hover_text("Tap at least 3 times in tempo");
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
            ui.add(
                number_field(bpm)
                    .range(20.0..=400.0)
                    .step(5.0)
                    .drag_step(1.0)
                    .decimals(0)
                    .font(theme::mono(28.0))
                    .suffix("bpm")
                    .id_salt("toolbar_bpm"),
            )
            .on_hover_text("Drag up / down (Shift ×10), or click to type (↑ / ↓: ±5, Shift ±50)");
        });
        *bpm != before
    }

    /// The tempo: the [`Toolbar::bpm`] readout, or with a tempo ramp, start → target
    /// with a short summary of the ramp below.
    pub fn tempo(&mut self, bpm: &mut f32, ramp: &mut TempoRamp) -> bool {
        if !ramp.enabled {
            return self.bpm(bpm);
        }
        let before = (*bpm, ramp.target);
        let summary = crate::widgets::session_panel::ramp_summary(*bpm, ramp);
        self.ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                fn field<'a>(v: &'a mut f32, salt: &str) -> controls::NumberField<'a, f32> {
                    number_field(v)
                        .range(20.0..=400.0)
                        .step(5.0)
                        .drag_step(1.0)
                        .decimals(0)
                        .font(theme::mono(20.0))
                        .id_salt(salt.to_owned())
                }
                ui.add(field(bpm, "toolbar_bpm"))
                    .on_hover_text("Start tempo: drag up / down, or click to type");
                ui.label(RichText::new(icons::ARROW_RIGHT).color(theme::dim()));
                ui.add(field(&mut ramp.target, "toolbar_bpm_target").suffix("bpm"))
                    .on_hover_text("Target tempo of the ramp");
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    RichText::new(icons::TRAY_ARROW_UP)
                        .font(theme::prop(13.0))
                        .color(theme::green()),
                );
                ui.label(
                    RichText::new(summary)
                        .font(theme::mono(13.0))
                        .color(theme::text()),
                );
            });
        });
        (*bpm, ramp.target) != before
    }

    /// Compact chips naming active options (icon + short label, full text on hover).
    /// Returns true when one was clicked.
    pub fn chips(&mut self, chips: &[Chip]) -> bool {
        let ui = &mut *self.ui;
        let mut clicked = false;
        for (i, c) in chips.iter().enumerate() {
            if i > 0 {
                ui.add_space(4.0);
            }
            let color = if ui.is_enabled() {
                theme::amber()
            } else {
                theme::dim()
            };
            let galley = ui.painter().layout_no_wrap(
                format!("{} {}", c.icon, c.label),
                theme::mono(12.0),
                color,
            );
            let size = galley.size() + vec2(12.0, 8.0);
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            let resp = resp.on_hover_text(&c.tooltip);
            if ui.is_rect_visible(rect) {
                let fill = color.gamma_multiply(if resp.hovered() { 0.22 } else { 0.1 });
                ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
                ui.painter()
                    .galley(rect.center() - galley.size() / 2.0, galley, color);
            }
            clicked |= resp.clicked();
        }
        clicked
    }

    /// Opens the [`Toolbar::menu`] with this `id_salt` (call before drawing the menu).
    pub fn open_menu(&mut self, id_salt: &str) {
        let id = self.ui.make_persistent_id(id_salt);
        self.ui.data_mut(|d| d.insert_temp(id, true));
    }

    /// The time signature, as a "4/4" dropdown with common signatures and custom fields.
    pub fn time_sig(&mut self, sig: &mut TimeSignature) -> bool {
        let before = *sig;
        let text = RichText::new(format!("{}/{}", sig.num, sig.den)).font(theme::mono(20.0));
        controls::dropdown(self.ui, "toolbar_time_sig", text, |ui| {
            for (n, d) in [
                (2, 4),
                (3, 4),
                (4, 4),
                (5, 4),
                (6, 8),
                (7, 8),
                (9, 8),
                (12, 8),
            ] {
                if ui
                    .add(selectable(sig.num == n && sig.den == d, format!("{n}/{d}")))
                    .clicked()
                {
                    *sig = TimeSignature { num: n, den: d };
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(number_field(&mut sig.num).range(1..=15).id_salt("ts_num"));
                ui.label("/");
                controls::dropdown_value(ui, "ts_den", &mut sig.den, [2, 4, 8, 16], |d| {
                    d.to_string()
                });
            });
        });
        *sig != before
    }

    /// The key: a tonic grid and the modes.
    pub fn key(&mut self, key: &mut Scale) -> bool {
        const ROOTS: [&str; 12] = [
            "C", "C#/Db", "D", "D#/Eb", "E", "F", "F#/Gb", "G", "G#/Ab", "A", "A#/Bb", "B",
        ];
        let before = *key;
        let text = RichText::new(key.short_name()).font(theme::mono(20.0));
        controls::dropdown(self.ui, "toolbar_key", text, |ui| {
            ui.set_min_width(230.0);
            egui::Grid::new("key_roots")
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    for (pc, name) in ROOTS.iter().enumerate() {
                        let pc = pc as u8;
                        if ui.add(selectable(key.root == pc, *name)).clicked() {
                            key.root = pc;
                        }
                        if pc % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
            ui.separator();
            for m in Mode::ALL {
                if ui.add(selectable(key.mode == m, m.label())).clicked() {
                    key.mode = m;
                }
            }
        })
        .0
        .on_hover_text("Key: sets the score's key signature (and the chords in Basic chords)");
        *key != before
    }

    /// An icon button with a caret that opens a panel of options below it. The open
    /// panel and the button form one shape: the panel with a tab on top. `active`
    /// highlights the icon (e.g. when any of the options is on).
    pub fn menu<R>(
        &mut self,
        id_salt: &str,
        icon: &str,
        tooltip: &str,
        active: bool,
        width: f32,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
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
        let galley = ui
            .painter()
            .layout_no_wrap(label.clone(), font.clone(), color);
        let (rect, resp) = ui.allocate_exact_size(
            vec2(galley.size().x + 2.0 * MENU_PADDING, BUTTON_HEIGHT),
            egui::Sense::click(),
        );
        let resp = resp.on_hover_text(tooltip);
        if ui.is_rect_visible(rect) {
            let c = if resp.hovered() && ui.is_enabled() {
                theme::text()
            } else {
                color
            };
            ui.painter()
                .galley(rect.center() - galley.size() / 2.0, galley, c);
        }
        if resp.clicked() {
            open = !open;
        }
        // While one of the panel's own dropdowns is open, a click is that dropdown's.
        let child_open = egui::Popup::is_any_open(ui.ctx());
        let behavior = if child_open {
            egui::PopupCloseBehavior::IgnoreClicks
        } else {
            egui::PopupCloseBehavior::CloseOnClickOutside
        };
        let tab = resp
            .rect
            .with_min_y(resp.rect.top() - 2.0)
            .with_max_y(resp.rect.bottom() + 6.0);
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
            painter.set(
                sheet,
                Shape::Vec(vec![
                    Shape::rect_filled(p, 0.0, fill),
                    Shape::rect_filled(tab, 0.0, fill),
                    Shape::closed_line(outline, stroke),
                ]),
            );
            // The button is under the sheet; draw its label again on the tab.
            painter.text(
                resp.rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                font,
                color,
            );
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
    (taps.len() >= 3)
        .then(|| 60.0 * (taps.len() - 1) as f32 / (taps[taps.len() - 1] - taps[0]) as f32)
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
