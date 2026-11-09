//! The basic form controls: no boxes, just a dotted underline that marks a value as
//! editable (text, number and dropdown fields), and a plain square checkbox.

use egui::emath::Numeric;
use egui::{
    Align, FontId, Id, Key, Modifiers, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
    Widget, WidgetText, vec2,
};

use crate::widgets::button::selectable;
use crate::{icons, theme};

/// The underline under an editable value: dotted at rest, solid while hovered or edited.
fn underline(ui: &Ui, rect: Rect, active: bool) {
    let y = rect.bottom() + 1.0;
    let color = if !ui.is_enabled() {
        theme::dim().gamma_multiply(0.6)
    } else {
        theme::cyan()
    };
    if active {
        ui.painter()
            .hline(rect.x_range(), y, Stroke::new(1.0, color));
    } else {
        let pts = [Pos2::new(rect.left() + 1.0, y), Pos2::new(rect.right(), y)];
        ui.painter()
            .extend(Shape::dotted_line(&pts, color, 3.0, 0.75));
    }
}

/// Removes the "changed" flag (set by typing), so callers only see committed changes.
fn clear_changed(mut r: Response) -> Response {
    r.flags.remove(egui::response::Flags::CHANGED);
    r
}

fn value_color(ui: &Ui) -> egui::Color32 {
    if ui.is_enabled() {
        theme::cyan()
    } else {
        theme::dim()
    }
}

// ---------------------------------------------------------------------------------------
// Checkbox

/// A square checkbox (an X when on) with an optional label; the label toggles it too.
pub fn checkbox(ui: &mut Ui, value: &mut bool, label: impl Into<WidgetText>) -> Response {
    Checkbox {
        value,
        label: label.into(),
    }
    .ui(ui)
}

pub struct Checkbox<'a> {
    value: &'a mut bool,
    label: WidgetText,
}

impl Checkbox<'_> {
    pub fn new(value: &mut bool, label: impl Into<WidgetText>) -> Checkbox<'_> {
        Checkbox {
            value,
            label: label.into(),
        }
    }
}

impl Widget for Checkbox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let box_size = (ui.text_style_height(&egui::TextStyle::Body) * 0.8).round();
        let gap = 8.0;
        let galley = (!self.label.is_empty()).then(|| {
            self.label.into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Body,
            )
        });
        let text_w = galley.as_ref().map_or(0.0, |g| g.size().x + gap);
        let h = ui.spacing().interact_size.y.max(box_size);
        let (rect, mut resp) = ui.allocate_exact_size(vec2(box_size + text_w, h), Sense::click());
        if resp.clicked() {
            *self.value = !*self.value;
            resp.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let b = Rect::from_center_size(
                Pos2::new(rect.left() + box_size / 2.0, rect.center().y),
                vec2(box_size, box_size),
            );
            let color = value_color(ui);
            let width = if resp.hovered() { 2.0 } else { 1.5 };
            let p = ui.painter();
            p.rect_stroke(b, 0.0, Stroke::new(width, color), StrokeKind::Inside);
            if *self.value {
                let i = b.shrink(3.5);
                let s = Stroke::new(2.0, color);
                p.line_segment([i.left_top(), i.right_bottom()], s);
                p.line_segment([i.left_bottom(), i.right_top()], s);
            }
            if let Some(g) = galley {
                let text_color = if ui.is_enabled() {
                    theme::text()
                } else {
                    theme::dim()
                };
                p.galley(
                    Pos2::new(b.right() + gap, rect.center().y - g.size().y / 2.0),
                    g,
                    text_color,
                );
            }
        }
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *self.value, "")
        });
        resp
    }
}

// ---------------------------------------------------------------------------------------
// Text field

/// A frameless single-line text field with a dotted underline.
pub struct TextField<'a> {
    text: &'a mut String,
    width: Option<f32>,
    align: Align,
    font: Option<FontId>,
    hint: Option<String>,
}

pub fn text_field(text: &mut String) -> TextField<'_> {
    TextField {
        text,
        width: None,
        align: Align::Min,
        font: None,
        hint: None,
    }
}

impl TextField<'_> {
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn font(mut self, f: FontId) -> Self {
        self.font = Some(f);
        self
    }
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }
}

impl Widget for TextField<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let mut te = egui::TextEdit::singleline(self.text)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::symmetric(1, 1))
            .horizontal_align(self.align)
            .text_color(value_color(ui))
            .desired_width(self.width.unwrap_or(160.0));
        if let Some(f) = self.font {
            te = te.font(f);
        }
        if let Some(h) = self.hint {
            te = te.hint_text(h);
        }
        let r = ui.add(te);
        underline(ui, r.rect, r.has_focus() || r.hovered());
        r
    }
}

// ---------------------------------------------------------------------------------------
// Number field

/// Pixels of vertical drag per step of a [`NumberField`].
const DRAG_PX: f32 = 6.0;

/// A number on a dotted underline. Drag it up / down to change it (Shift: ×10), or click
/// to type (Enter or leaving the field commits, Esc reverts); ↑ / ↓ step the value while
/// editing (Shift: ×10).
pub struct NumberField<'a, T: Numeric> {
    value: &'a mut T,
    min: f64,
    max: f64,
    step: f64,
    /// Value change per drag notch; `None` disables dragging.
    drag_step: Option<f64>,
    decimals: usize,
    suffix: Option<String>,
    font: Option<FontId>,
    width: Option<f32>,
    align: Align,
    id: Option<Id>,
}

pub fn number_field<T: Numeric>(value: &mut T) -> NumberField<'_, T> {
    NumberField {
        value,
        min: T::MIN.to_f64(),
        max: T::MAX.to_f64(),
        step: 1.0,
        drag_step: Some(f64::NAN),
        decimals: if T::INTEGRAL { 0 } else { 1 },
        suffix: None,
        font: None,
        width: None,
        align: Align::Center,
        id: None,
    }
}

impl<T: Numeric> NumberField<'_, T> {
    pub fn range(mut self, r: std::ops::RangeInclusive<T>) -> Self {
        self.min = r.start().to_f64();
        self.max = r.end().to_f64();
        self
    }
    pub fn step(mut self, s: f64) -> Self {
        self.step = s;
        self
    }
    /// Value change per drag notch (defaults to the ↑ / ↓ step).
    pub fn drag_step(mut self, s: f64) -> Self {
        self.drag_step = Some(s);
        self
    }
    /// Only typing changes the value.
    pub fn no_drag(mut self) -> Self {
        self.drag_step = None;
        self
    }
    pub fn decimals(mut self, d: usize) -> Self {
        self.decimals = d;
        self
    }
    pub fn suffix(mut self, s: impl Into<String>) -> Self {
        self.suffix = Some(s.into());
        self
    }
    pub fn font(mut self, f: FontId) -> Self {
        self.font = Some(f);
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn id_salt(mut self, salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = Some(Id::new(salt));
        self
    }

    fn format(&self, v: f64) -> String {
        format!("{:.*}", self.decimals, v)
    }

    fn clamp(&self, v: f64) -> f64 {
        let v = if T::INTEGRAL { v.round() } else { v };
        v.clamp(self.min, self.max)
    }
}

impl<T: Numeric> Widget for NumberField<'_, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let id = self
            .id
            .map_or_else(|| ui.next_auto_id(), |i| ui.make_persistent_id(i));
        let current = self.value.to_f64();
        let focused = ui.memory(|m| m.has_focus(id));
        let mut changed = false;
        let mut new_value = current;

        let mut buf: String = if focused {
            ui.data(|d| d.get_temp::<String>(id))
                .unwrap_or_else(|| self.format(current))
        } else {
            self.format(current)
        };
        if focused {
            let delta = ui.input_mut(|i| {
                let big = 10
                    * (i.consume_key(Modifiers::SHIFT, Key::ArrowUp) as i32
                        - i.consume_key(Modifiers::SHIFT, Key::ArrowDown) as i32);
                let small = i.consume_key(Modifiers::NONE, Key::ArrowUp) as i32
                    - i.consume_key(Modifiers::NONE, Key::ArrowDown) as i32;
                big + small
            });
            if delta != 0 {
                let base = buf.trim().parse::<f64>().unwrap_or(current);
                new_value = self.clamp(base + delta as f64 * self.step);
                buf = self.format(new_value);
                changed = true;
            }
        }

        // Width: fits the widest value in range and the current text (or a fixed width).
        let font = self
            .font
            .clone()
            .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
        let width = self.width.unwrap_or_else(|| {
            let lo = self.format(if self.min.is_finite() && self.min > -1e9 {
                self.min
            } else {
                -999.0
            });
            let hi = self.format(if self.max.is_finite() && self.max < 1e9 {
                self.max
            } else {
                9999.0
            });
            let w = |s: String| {
                ui.fonts_mut(|f| {
                    f.layout_no_wrap(s, font.clone(), egui::Color32::WHITE)
                        .size()
                        .x
                })
            };
            // Wide enough for the range and for the current value / typed text (large
            // ranges, e.g. seeds, fall back to a 4-digit estimate).
            w(lo).max(w(hi)).max(w(buf.clone())) + 4.0
        });

        const MARGIN: f32 = 2.0;

        let drag_step = self
            .drag_step
            .map(|s| if s.is_nan() { self.step } else { s });
        let editing = focused || drag_step.is_none();
        let inner = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let r = if editing {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut buf)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::symmetric(1, 1))
                        .horizontal_align(self.align)
                        .font(font.clone())
                        .text_color(value_color(ui))
                        .desired_width(width + MARGIN),
                );
                underline(ui, r.rect, r.has_focus() || r.hovered());
                r
            } else {
                // Not editing: the value as a drag handle (same size as the text edit).
                let line_h =
                    ui.fonts_mut(|f| f.row_height(&font)) + ui.spacing().extra_text_line_spacing;
                let (rect, _) =
                    ui.allocate_exact_size(vec2(width + MARGIN, line_h), Sense::hover());
                let r = ui.interact(rect, id, Sense::click_and_drag());
                if ui.is_rect_visible(rect) {
                    let galley =
                        ui.painter()
                            .layout_no_wrap(buf.clone(), font.clone(), value_color(ui));
                    let x = match self.align {
                        Align::Min => rect.left() + 1.0,
                        Align::Center => rect.center().x - galley.size().x / 2.0,
                        Align::Max => rect.right() - 1.0 - galley.size().x,
                    };
                    let y = rect.center().y - galley.size().y / 2.0;
                    ui.painter()
                        .galley(Pos2::new(x, y), galley, value_color(ui));
                }
                underline(ui, rect, r.hovered() || r.dragged());
                r.on_hover_cursor(egui::CursorIcon::ResizeVertical)
            };
            if let Some(s) = &self.suffix {
                let rt = egui::RichText::new(s).color(theme::dim());
                ui.label(rt.font(font));
            }
            r
        });
        let r = inner.inner;

        if editing {
            if r.lost_focus() {
                if !ui.input(|i| i.key_pressed(Key::Escape))
                    && let Ok(v) = buf.trim().replace(',', ".").parse::<f64>()
                {
                    new_value = self.clamp(v);
                    changed |= new_value != current;
                }
                ui.data_mut(|d| d.remove::<String>(id));
            } else if r.has_focus() {
                ui.data_mut(|d| d.insert_temp(id, buf));
            }
        } else if let Some(drag_step) = drag_step {
            let acc_id = id.with("drag_acc");
            if r.drag_started() {
                ui.data_mut(|d| d.insert_temp(acc_id, 0.0f32));
            }
            if r.dragged() {
                // Up increases; one notch per DRAG_PX pixels.
                let mut acc: f32 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(0.0);
                acc -= r.drag_delta().y;
                let notches = (acc / DRAG_PX).trunc();
                if notches != 0.0 {
                    acc -= notches * DRAG_PX;
                    let mult = if ui.input(|i| i.modifiers.shift) {
                        10.0
                    } else {
                        1.0
                    };
                    new_value = self.clamp(current + notches as f64 * drag_step * mult);
                    changed = true;
                }
                ui.data_mut(|d| d.insert_temp(acc_id, acc));
            } else if r.clicked() {
                ui.memory_mut(|m| m.request_focus(id));
            }
        }

        let mut r = clear_changed(r);
        if changed && new_value != current {
            *self.value = T::from_f64(new_value);
            r.mark_changed();
        }
        r
    }
}

// ---------------------------------------------------------------------------------------
// Dropdown

/// Draws the closed dropdown: its text and a caret on a dotted underline.
fn dropdown_field(ui: &mut Ui, id: Id, text: WidgetText, min_width: f32) -> Response {
    let max_text = if min_width > 0.0 {
        min_width - 24.0
    } else {
        ui.available_width()
    };
    let galley = text.into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        max_text.max(40.0),
        egui::TextStyle::Button,
    );
    let caret = ui.fonts_mut(|f| {
        f.layout_no_wrap(
            icons::CARET_DOWN.to_owned(),
            egui::TextStyle::Small.resolve(ui.style()),
            egui::Color32::WHITE,
        )
    });
    let w = (galley.size().x + 6.0 + caret.size().x).max(min_width);
    let h = ui.spacing().interact_size.y;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    if ui.is_rect_visible(rect) {
        let color = value_color(ui);
        let text_rect = Rect::from_min_size(
            Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0),
            galley.size(),
        );
        let p = ui.painter();
        p.galley(text_rect.min, galley, color);
        p.text(
            Pos2::new(rect.right(), rect.center().y),
            egui::Align2::RIGHT_CENTER,
            icons::CARET_DOWN,
            egui::TextStyle::Small.resolve(ui.style()),
            color,
        );
        let line = Rect::from_x_y_ranges(rect.x_range(), text_rect.y_range());
        underline(
            ui,
            line,
            resp.hovered()
                || egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp)),
        );
    }
    resp
}

/// A dropdown: the selected text with a caret; clicking opens a list below it.
pub fn dropdown<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected: impl Into<WidgetText>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (Response, Option<R>) {
    dropdown_sized(ui, id_salt, selected, 0.0, add_contents)
}

/// A [`dropdown`] at least `width` wide (its text is truncated to fit).
pub fn dropdown_sized<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected: impl Into<WidgetText>,
    width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (Response, Option<R>) {
    let id = ui.make_persistent_id(id_salt);
    let resp = dropdown_field(ui, id, selected.into(), width);
    let width = resp.rect.width().max(80.0);
    let inner = egui::Popup::menu(&resp).width(width).show(|ui| {
        ui.set_min_width(width);
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, add_contents)
            .inner
    });
    (resp, inner.map(|i| i.inner))
}

/// A dropdown choosing one of `options`. The response is marked changed on a new pick.
pub fn dropdown_value<T: PartialEq + Clone>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut T,
    options: impl IntoIterator<Item = T>,
    label: impl Fn(&T) -> String,
) -> Response {
    let mut picked = None;
    let (mut resp, _) = dropdown(ui, id_salt, label(value), |ui| {
        for o in options {
            if ui.add(selectable(*value == o, label(&o))).clicked() {
                picked = Some(o);
            }
        }
    });
    if let Some(o) = picked
        && o != *value
    {
        *value = o;
        resp.mark_changed();
    }
    resp
}

// ---------------------------------------------------------------------------------------
// Layout helpers

/// Lays out `add` (left to right) flush with the right edge of the remaining row space.
/// Uses the width measured on the previous frame, so the first frame may be off.
pub fn right_aligned<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let id = ui.make_persistent_id(id_salt).with("right_aligned");
    let w: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let spacing = ui.spacing().item_spacing.x;
    let pad = (ui.available_width() - w - spacing).max(0.0);
    if pad > 0.0 {
        ui.add_space(pad);
    }
    let r = ui.horizontal(add);
    let new_w = r.response.rect.width();
    if (new_w - w).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, new_w));
        ui.ctx().request_repaint();
    }
    r
}

/// A form row: icon and label on the left, the control flush right, joined by a dotted
/// leader. Used in the toolbar dropdowns.
pub fn form_row<R>(ui: &mut Ui, icon: &str, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let text_color = if ui.is_enabled() {
            theme::text()
        } else {
            theme::dim()
        };
        let icon_w = ui.text_style_height(&egui::TextStyle::Body) * 1.2;
        let (ir, _) =
            ui.allocate_exact_size(vec2(icon_w, ui.spacing().interact_size.y), Sense::hover());
        if !icon.is_empty() {
            ui.painter().text(
                ir.center(),
                egui::Align2::CENTER_CENTER,
                icon,
                egui::TextStyle::Body.resolve(ui.style()),
                theme::dim(),
            );
        }
        let l = ui.label(egui::RichText::new(label).color(text_color));
        let r = right_aligned(ui, ("form_row", label), add);
        let (x0, x1) = (l.rect.right() + 6.0, r.response.rect.left() - 8.0);
        if x1 - x0 > 8.0 {
            let y = l.rect.center().y + 3.0;
            ui.painter().extend(Shape::dotted_line(
                &[Pos2::new(x0, y), Pos2::new(x1, y)],
                theme::grid(),
                4.0,
                0.75,
            ));
        }
        r.inner
    })
    .inner
}

// ---------------------------------------------------------------------------------------
// Slider with a number field

/// A slider followed by its value as an underlined [`NumberField`] (instead of egui's
/// boxed value).
pub struct SliderField<'a, T: Numeric> {
    value: &'a mut T,
    range: std::ops::RangeInclusive<T>,
    step: Option<f64>,
    decimals: Option<usize>,
    suffix: Option<String>,
    logarithmic: bool,
}

pub fn slider_field<T: Numeric>(
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
) -> SliderField<'_, T> {
    SliderField {
        value,
        range,
        step: None,
        decimals: None,
        suffix: None,
        logarithmic: false,
    }
}

impl<T: Numeric> SliderField<'_, T> {
    /// A logarithmic slider: finer at the low end of the range.
    pub fn logarithmic(mut self) -> Self {
        self.logarithmic = true;
        self
    }
    /// Slider step and ↑ / ↓ step of the field.
    pub fn step_by(mut self, s: f64) -> Self {
        self.step = Some(s);
        self
    }
    pub fn decimals(mut self, d: usize) -> Self {
        self.decimals = Some(d);
        self
    }
    pub fn suffix(mut self, s: impl Into<String>) -> Self {
        self.suffix = Some(s.into());
        self
    }
}

impl<T: Numeric> Widget for SliderField<'_, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let span = self.range.end().to_f64() - self.range.start().to_f64();
        let step = self.step.unwrap_or(if T::INTEGRAL || span > 20.0 {
            1.0
        } else if span > 2.0 {
            0.5
        } else {
            0.05
        });
        let decimals = self.decimals.unwrap_or(if T::INTEGRAL || step >= 1.0 {
            0
        } else if step >= 0.1 {
            1
        } else {
            2
        });
        ui.horizontal(|ui| {
            let mut slider = egui::Slider::new(self.value, self.range.clone())
                .show_value(false)
                .logarithmic(self.logarithmic);
            if let Some(s) = self.step {
                slider = slider.step_by(s);
            }
            let r = ui.add(slider);
            let mut field = number_field(self.value)
                .range(self.range)
                .step(step)
                .decimals(decimals)
                .align(Align::Max);
            if let Some(s) = self.suffix {
                field = field.suffix(s);
            }
            r.union(ui.add(field))
        })
        .inner
    }
}
