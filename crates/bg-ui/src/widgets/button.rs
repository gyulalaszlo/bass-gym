//! The push button and the selectable item.
//!
//! A button is a dimly filled, rounded rectangle with its text in the button colour; on
//! hover the colours flip (solid button colour, text in the dim fill). A selectable item
//! shows just its text at rest, the regular dim button when hovered, and the flipped
//! (hovered-button) look when selected. Sizes never depend on the state, so hovering
//! never moves the layout.

use std::sync::Arc;

use egui::{
    Color32, CursorIcon, FontId, Galley, Pos2, Rect, Response, Sense, TextStyle, Ui, Vec2, Widget,
    WidgetInfo, WidgetText, WidgetType,
};

use crate::theme;

/// Text, colour and size shared by [`Button`] and [`Selectable`].
struct Look {
    text: String,
    color: Option<Color32>,
    font: Option<FontId>,
    strong: bool,
    small: bool,
    min_size: Vec2,
    /// Centre the text (selectables are left-aligned by default).
    centred: bool,
}

impl Look {
    fn new(text: String) -> Self {
        Self {
            text,
            color: None,
            font: None,
            strong: false,
            small: false,
            min_size: Vec2::ZERO,
            centred: false,
        }
    }

    fn color(&self) -> Color32 {
        self.color.unwrap_or_else(theme::cyan)
    }

    /// Lays out the text and allocates the widget. With `fill_width` the item takes the
    /// full width of a justified layout (e.g. a popup list).
    fn allocate(&self, ui: &mut Ui, fill_width: bool) -> (Rect, Response, Arc<Galley>) {
        // Placeholder colour: painted in the state's text colour.
        let mut rich = egui::RichText::new(&self.text).color(Color32::PLACEHOLDER);
        if let Some(f) = &self.font {
            rich = rich.font(f.clone());
        } else if self.small {
            rich = rich.text_style(TextStyle::Body);
        }
        if self.strong {
            rich = rich.strong();
        }
        let galley = WidgetText::from(rich).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Button,
        );

        let mut pad = ui.spacing().button_padding;
        if self.small {
            pad.y = 0.0;
        }
        let mut size = galley.size() + 2.0 * pad;
        if !self.small {
            size.y = size.y.max(ui.spacing().interact_size.y);
        }
        let size = size.max(self.min_size);
        let (rect, resp) = if fill_width {
            ui.allocate_at_least(size, Sense::click())
        } else {
            ui.allocate_exact_size(size, Sense::click())
        };
        (rect, resp, galley)
    }

    /// Paints the (optional) fill and the text, centred or left-aligned after the padding.
    fn paint(
        ui: &Ui,
        rect: Rect,
        galley: Arc<Galley>,
        fill: Option<Color32>,
        text: Color32,
        centred: bool,
    ) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        let p = ui.painter();
        if let Some(fill) = fill {
            p.rect_filled(rect, theme::button_rounding(), fill);
        }
        let y = rect.center().y - galley.size().y / 2.0;
        let x = if centred {
            rect.center().x - galley.size().x / 2.0
        } else {
            rect.left() + ui.spacing().button_padding.x
        };
        p.galley(Pos2::new(x, y), galley, text);
    }
}

macro_rules! look_builders {
    () => {
        /// The colour (default: [`theme::cyan`]).
        pub fn color(mut self, c: Color32) -> Self {
            self.look.color = Some(c);
            self
        }
        pub fn font(mut self, f: FontId) -> Self {
            self.look.font = Some(f);
            self
        }
        pub fn strong(mut self) -> Self {
            self.look.strong = true;
            self
        }
        /// Body-size text and no vertical padding, like egui's `small_button`.
        pub fn small(mut self) -> Self {
            self.look.small = true;
            self
        }
        pub fn min_size(mut self, s: impl Into<Vec2>) -> Self {
            self.look.min_size = s.into();
            self
        }
        /// Centre the text in the widget.
        pub fn centred(mut self) -> Self {
            self.look.centred = true;
            self
        }
    };
}

fn pointer_cursor(ui: &Ui, resp: &Response) {
    if resp.hovered() && ui.is_enabled() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

// ---------------------------------------------------------------------------------------
// Button

/// A button showing `text` (icons included) in the theme's selectable colour.
pub fn button(text: impl Into<String>) -> Button {
    Button {
        look: Look::new(text.into()),
    }
}

#[must_use = "Put the button in a ui with `ui.add(button)`"]
pub struct Button {
    look: Look,
}

impl Button {
    look_builders!();
}

impl Widget for Button {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, resp, galley) = self.look.allocate(ui, false);
        let color = self.look.color();
        let base = theme::button_fill(color);
        let (fill, text) = if !ui.is_enabled() {
            (base.gamma_multiply(0.5), theme::dim())
        } else if resp.hovered() || resp.is_pointer_button_down_on() {
            (color, base)
        } else {
            (base, color)
        };
        Look::paint(ui, rect, galley, Some(fill), text, true);
        pointer_cursor(ui, &resp);
        resp.widget_info(|| {
            WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), &self.look.text)
        });
        resp
    }
}

// ---------------------------------------------------------------------------------------
// Selectable

/// A selectable item (list entry, option chip, toggle): text only at rest, a dim button
/// when hovered, a solid button when selected. Fills the width of justified layouts.
pub fn selectable(selected: bool, text: impl Into<String>) -> Selectable {
    Selectable {
        look: Look::new(text.into()),
        selected,
    }
}

#[must_use = "Put the item in a ui with `ui.add(selectable)`"]
pub struct Selectable {
    look: Look,
    selected: bool,
}

impl Selectable {
    look_builders!();
}

impl Widget for Selectable {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, resp, galley) = self.look.allocate(ui, true);
        let color = self.look.color();
        let base = theme::button_fill(color);
        let hovered = resp.hovered() || resp.is_pointer_button_down_on();
        let (fill, text) = match (ui.is_enabled(), self.selected) {
            (false, true) => (Some(base), theme::dim()),
            (false, false) => (None, theme::dim()),
            (true, true) => (Some(color), base),
            (true, false) if hovered => (Some(base), color),
            (true, false) => (None, color),
        };
        Look::paint(ui, rect, galley, fill, text, self.look.centred);
        pointer_cursor(ui, &resp);
        resp.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::SelectableLabel,
                ui.is_enabled(),
                self.selected,
                &self.look.text,
            )
        });
        resp
    }
}

/// A [`selectable`] that sets `current` to `value` when clicked (marked changed if it
/// was different), like egui's `selectable_value`.
pub fn selectable_value<T: PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    value: T,
    text: impl Into<String>,
) -> Response {
    let mut resp = ui.add(selectable(*current == value, text));
    if resp.clicked() && *current != value {
        *current = value;
        resp.mark_changed();
    }
    resp
}
