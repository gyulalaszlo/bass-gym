//! The Exercise Control Toolbar: a fixed-height strip above the exercise area that stays
//! visible while the exercise scrolls. Exercises fill it through
//! [`crate::ExerciseType::toolbar`] using the building blocks here, so every exercise
//! shares the same look: start / stop, tempo, time signature, option menus and a live
//! status. Items are separated by [`Gap`]s that share out the free width, so the
//! toolbar always spans the full width with a balanced layout.
//!
//! A tier-0 container: its look is a [`Theme`] given to [`show_toolbar`] (the shell
//! holds it), which every building block reads through the [`Toolbar`].

use std::cell::Cell;

use bg_core::TimeSignature;
use bg_core::harmony::{Mode, Scale};
use bg_core::session::TempoRamp;
use egui::{Align, Color32, FontId, Layout, Pos2, Rect, RichText, Shape, Stroke, Ui, Vec2, vec2};

use crate::icons;
use crate::theme::{Tokens, caption};
use crate::widgets::button::{button, selectable};
use crate::widgets::controls::{self, number_field};

/// How the toolbar looks.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// The row, and its buttons.
    pub height: f32,
    pub button_height: f32,
    /// Each [`Gap`]'s minimum width and its share of the free width (Small, Medium,
    /// Large).
    pub gaps: [(f32, f32); 3],
    /// Space between the items of a status shown in a gap.
    pub status_spacing: f32,
    pub start: Color32,
    pub stop: Color32,
    pub transport_font: FontId,
    pub transport_width: f32,
    pub results_font: FontId,
    pub bpm_font: FontId,
    pub ramp_font: FontId,
    pub ramp_icon_font: FontId,
    pub ramp_icon: Color32,
    pub summary_font: FontId,
    pub summary: Color32,
    /// Secondary marks (the ramp's arrow) and disabled items.
    pub dim: Color32,
    pub chip_font: FontId,
    pub chip: Color32,
    pub chip_padding: Vec2,
    pub chip_gap: f32,
    /// A chip's fill strength, at rest and hovered.
    pub chip_fill: (f32, f32),
    /// Menu buttons: normal, active (an option is on), hovered; their caret, and the
    /// font of a menu's value label.
    pub menu: Color32,
    pub menu_active: Color32,
    pub menu_hover: Color32,
    pub menu_icon_font: FontId,
    pub menu_label_font: FontId,
    pub menu_padding: f32,
    pub sheet_fill: Color32,
    pub sheet_line: Stroke,
    pub sheet_margin: i8,
    /// Equal-width choices in a menu: gap and height; and the gap above a caption.
    pub choice_gap: f32,
    pub choice_height: f32,
    pub caption_gap: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            height: 48.0,
            button_height: 36.0,
            gaps: [(8.0, 1.0), (16.0, 2.0), (24.0, 6.0)],
            status_spacing: 10.0,
            start: t.green(),
            stop: t.amber(),
            transport_font: FontId::proportional(16.0),
            transport_width: 118.0,
            results_font: FontId::proportional(18.0),
            bpm_font: t.mono(28.0),
            ramp_font: t.mono(20.0),
            ramp_icon_font: t.prop(13.0),
            ramp_icon: t.green(),
            summary_font: t.mono(13.0),
            summary: t.text(),
            dim: t.dim(),
            chip_font: t.mono(12.0),
            chip: t.amber(),
            chip_padding: vec2(12.0, 8.0),
            chip_gap: 4.0,
            chip_fill: (0.1, 0.22),
            menu: t.cyan(),
            menu_active: t.green(),
            menu_hover: t.text(),
            menu_icon_font: FontId::proportional(20.0),
            menu_label_font: t.mono(20.0),
            menu_padding: 12.0,
            sheet_fill: t.panel(),
            sheet_line: Stroke::new(1.0, t.line()),
            sheet_margin: 14,
            choice_gap: 6.0,
            choice_height: 32.0,
            caption_gap: 10.0,
        }
    }
}

impl Theme {
    fn gap(&self, gap: Gap) -> (f32, f32) {
        self.gaps[gap as usize]
    }
}

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
    theme: &'a Theme,
    tally: &'a GapTally,
    /// Free width to share between the gaps (from the previous frame).
    extra: f32,
    weights: f32,
}

/// Shows the toolbar across the full width.
pub fn show_toolbar<R>(ui: &mut Ui, theme: &Theme, add: impl FnOnce(&mut Toolbar) -> R) -> R {
    let width = ui.available_width();
    let id = ui.id().with("exercise_toolbar");
    let last: Measured = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let tally = GapTally::default();
    let (r, now) = ui
        .allocate_ui_with_layout(
            vec2(width, theme.height),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_size(vec2(width, theme.height));
                ui.spacing_mut().item_spacing.x = 0.0;
                let left = ui.cursor().left();
                let extra = (width - last.items - last.min).max(0.0);
                let r = add(&mut Toolbar {
                    ui,
                    theme,
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
    /// The toolbar's look, for building blocks added from outside.
    pub fn theme(&self) -> &Theme {
        self.theme
    }

    fn gap_width(&self, gap: Gap) -> f32 {
        let (min, weight) = self.theme.gap(gap);
        let share = if self.weights > 0.0 {
            self.extra * weight / self.weights
        } else {
            0.0
        };
        min + share
    }

    fn tally(&self, gap: Gap, w: f32) {
        let (min, weight) = self.theme.gap(gap);
        let t = self.tally;
        t.width.set(t.width.get() + w);
        t.min.set(t.min.get() + min);
        t.weight.set(t.weight.get() + weight);
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
            .allocate_exact_size(vec2(w, self.theme.height), egui::Sense::hover());
        self.tally(gap, w);
        let inner = rect.shrink2(vec2(self.theme.gap(Gap::Medium).0, 0.0));
        let mut child = self.ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        child.set_clip_rect(inner.intersect(self.ui.clip_rect()));
        child.spacing_mut().item_spacing.x = self.theme.status_spacing;
        add(&mut child)
    }

    /// The underlying row, for exercise-specific controls.
    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    /// A group of items, enabled or not (e.g. settings are locked while a session runs).
    pub fn group<R>(&mut self, enabled: bool, add: impl FnOnce(&mut Toolbar) -> R) -> R {
        let (theme, tally, extra, weights) = (self.theme, self.tally, self.extra, self.weights);
        self.ui
            .add_enabled_ui(enabled, |ui| {
                add(&mut Toolbar {
                    ui,
                    theme,
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
        let (ui, t) = (&mut *self.ui, self.theme);
        let space = ui.input(|i| i.key_pressed(egui::Key::Space))
            && !ui.ctx().egui_wants_keyboard_input()
            && !crate::views::results::modal_open(ui.ctx());
        let (text, color) = if running {
            (format!("{}  STOP", icons::STOP), t.stop)
        } else {
            (format!("{}  START", icons::PLAY), t.start)
        };
        let clicked = ui
            .add(
                button(text)
                    .color(color)
                    .strong()
                    .font(t.transport_font.clone())
                    .min_size([t.transport_width, t.button_height]),
            )
            .on_hover_text("Space: start / stop")
            .clicked();
        clicked || space
    }

    /// A small button that (re)opens the last session's results; disabled while there
    /// are none. Returns true when clicked.
    pub fn results_button(&mut self, available: bool) -> bool {
        let (ui, t) = (&mut *self.ui, self.theme);
        ui.add_enabled_ui(available, |ui| {
            ui.add(
                button(icons::CHART_BAR)
                    .font(t.results_font.clone())
                    .min_size([t.button_height, t.button_height]),
            )
            .on_hover_text("Results of the last session")
            .on_disabled_hover_text("No results yet: run a session")
            .clicked()
        })
        .inner
    }

    /// − [large BPM readout] bpm +. The field steps by 5 with ↑ / ↓, the buttons by 1.
    pub fn bpm(&mut self, bpm: &mut f32) -> bool {
        let before = *bpm;
        let font = self.theme.bpm_font.clone();
        self.ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add(
                number_field(bpm)
                    .range(20.0..=400.0)
                    .step(5.0)
                    .drag_step(1.0)
                    .decimals(0)
                    .font(font)
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
        let t = self.theme;
        self.ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                fn field<'a>(
                    v: &'a mut f32,
                    font: &FontId,
                    salt: &str,
                ) -> controls::NumberField<'a, f32> {
                    number_field(v)
                        .range(20.0..=400.0)
                        .step(5.0)
                        .drag_step(1.0)
                        .decimals(0)
                        .font(font.clone())
                        .id_salt(salt.to_owned())
                }
                ui.add(field(bpm, &t.ramp_font, "toolbar_bpm"))
                    .on_hover_text("Start tempo: drag up / down, or click to type");
                ui.label(RichText::new(icons::ARROW_RIGHT).color(t.dim));
                ui.add(field(&mut ramp.target, &t.ramp_font, "toolbar_bpm_target").suffix("bpm"))
                    .on_hover_text("Target tempo of the ramp");
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    RichText::new(icons::TRAY_ARROW_UP)
                        .font(t.ramp_icon_font.clone())
                        .color(t.ramp_icon),
                );
                ui.label(
                    RichText::new(summary)
                        .font(t.summary_font.clone())
                        .color(t.summary),
                );
            });
        });
        (*bpm, ramp.target) != before
    }

    /// Compact chips naming active options (icon + short label, full text on hover).
    /// Returns true when one was clicked.
    pub fn chips(&mut self, chips: &[Chip]) -> bool {
        let (ui, t) = (&mut *self.ui, self.theme);
        let mut clicked = false;
        for (i, c) in chips.iter().enumerate() {
            if i > 0 {
                ui.add_space(t.chip_gap);
            }
            let color = if ui.is_enabled() { t.chip } else { t.dim };
            let galley = ui.painter().layout_no_wrap(
                format!("{} {}", c.icon, c.label),
                t.chip_font.clone(),
                color,
            );
            let size = galley.size() + t.chip_padding;
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            let resp = resp.on_hover_text(&c.tooltip);
            if ui.is_rect_visible(rect) {
                let (rest, hovered) = t.chip_fill;
                let fill = color.gamma_multiply(if resp.hovered() { hovered } else { rest });
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

    /// The time signature, as a "4/4" menu of common signatures and a custom one.
    pub fn time_sig(&mut self, sig: &mut TimeSignature) -> bool {
        const COMMON: [(u32, u32); 8] = [
            (2, 4),
            (3, 4),
            (4, 4),
            (5, 4),
            (6, 8),
            (7, 8),
            (9, 8),
            (12, 8),
        ];
        let before = *sig;
        let t = self.theme.clone();
        let label = (
            format!("{}/{}", sig.num, sig.den),
            t.menu_label_font.clone(),
        );
        self.menu_labelled(
            "toolbar_time_sig",
            label,
            "Time signature",
            false,
            300.0,
            |ui| {
                caption(ui, "Common");
                let items: Vec<_> = COMMON
                    .iter()
                    .map(|&(n, d)| (TimeSignature { num: n, den: d }, format!("{n}/{d}")))
                    .collect();
                choice_grid(ui, &t, "ts_common", 4, &items, sig);
                ui.add_space(t.caption_gap);
                caption(ui, "Custom");
                ui.horizontal(|ui| {
                    ui.add(number_field(&mut sig.num).range(1..=15).id_salt("ts_num"));
                    ui.label("/");
                    controls::dropdown_value(ui, "ts_den", &mut sig.den, [2, 4, 8, 16], |d| {
                        d.to_string()
                    });
                });
            },
        );
        *sig != before
    }

    /// The key, as a "E maj" menu of tonics and modes.
    pub fn key(&mut self, key: &mut Scale) -> bool {
        const ROOTS: [&str; 12] = [
            "C", "C#/Db", "D", "D#/Eb", "E", "F", "F#/Gb", "G", "G#/Ab", "A", "A#/Bb", "B",
        ];
        let before = *key;
        let t = self.theme.clone();
        let label = (key.short_name(), t.menu_label_font.clone());
        let tip = "Key: sets the score's key signature (and the chords in Basic chords)";
        self.menu_labelled("toolbar_key", label, tip, false, 340.0, |ui| {
            caption(ui, "Tonic");
            let roots: Vec<_> = (0..12u8).zip(ROOTS.map(String::from)).collect();
            choice_grid(ui, &t, "key_roots", 4, &roots, &mut key.root);
            ui.add_space(t.caption_gap);
            caption(ui, "Mode");
            let modes: Vec<_> = Mode::ALL.map(|m| (m, m.label().to_owned())).into();
            choice_grid(ui, &t, "key_modes", 2, &modes, &mut key.mode);
        });
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
        let label = (icon.to_owned(), self.theme.menu_icon_font.clone());
        self.menu_labelled(id_salt, label, tooltip, active, width, add)
    }

    /// A [`Toolbar::menu`] whose button shows `label` (text and font) before the caret,
    /// for settings whose current value is worth seeing, like the time signature.
    pub fn menu_labelled<R>(
        &mut self,
        id_salt: &str,
        (text, text_font): (String, egui::FontId),
        tooltip: &str,
        active: bool,
        width: f32,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
        let (ui, t) = (&mut *self.ui, self.theme);
        let id = ui.make_persistent_id(id_salt);
        let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let color = if !ui.is_enabled() {
            t.dim
        } else if active {
            t.menu_active
        } else {
            t.menu
        };
        // Drawn by hand so the label has a fixed horizontal margin (it becomes the tab).
        let ctx = ui.ctx().clone();
        let label = |color| {
            let mut job = egui::text::LayoutJob::default();
            let format = |font_id| egui::TextFormat {
                font_id,
                color,
                valign: Align::Center,
                ..Default::default()
            };
            job.append(&text, 0.0, format(text_font.clone()));
            job.append(
                &format!(" {}", icons::CARET_DOWN),
                0.0,
                format(t.menu_icon_font.clone()),
            );
            ctx.fonts_mut(|f| f.layout_job(job))
        };
        let galley = label(color);
        let (rect, resp) = ui.allocate_exact_size(
            vec2(galley.size().x + 2.0 * t.menu_padding, t.button_height),
            egui::Sense::click(),
        );
        let resp = resp.on_hover_text(tooltip);
        if ui.is_rect_visible(rect) {
            let galley = if resp.hovered() && ui.is_enabled() {
                label(t.menu_hover)
            } else {
                galley.clone()
            };
            ui.painter()
                .galley(rect.center() - galley.size() / 2.0, galley, color);
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
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::same(t.sheet_margin)))
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
            let (fill, stroke) = (t.sheet_fill, t.sheet_line);
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
            painter.galley(resp.rect.center() - galley.size() / 2.0, galley, color);
        }
        Some(r)
    }

    /// A thin vertical divider.
    pub fn separator(&mut self) {
        self.ui.separator();
    }
}

/// Equal-width, centred choices in `cols` columns, filling the width; sets `value` to
/// the one clicked.
fn choice_grid<T: PartialEq + Copy>(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: &str,
    cols: usize,
    items: &[(T, String)],
    value: &mut T,
) {
    let gap = theme.choice_gap;
    let cols = cols.max(1);
    let w = (ui.available_width() - gap * (cols - 1) as f32) / cols as f32;
    egui::Grid::new(id_salt).spacing([gap, gap]).show(ui, |ui| {
        for (i, (v, name)) in items.iter().enumerate() {
            let item = selectable(*value == *v, name.as_str())
                .centred()
                .min_size([w, theme.choice_height]);
            if ui.add(item).clicked() {
                *value = *v;
            }
            if i % cols == cols - 1 {
                ui.end_row();
            }
        }
    });
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
