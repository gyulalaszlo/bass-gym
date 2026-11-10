//! The push button and the selectable item.
//!
//! A button is a dimly filled, rounded rectangle with its text in the button colour; on
//! hover the colours flip (solid button colour, text in the dim fill). A selectable item
//! shows just its text at rest, the regular dim button when hovered, and the flipped
//! (hovered-button) look when selected. Sizes never depend on the state, so hovering
//! never moves the layout.
//!
//! The look comes from a [`Theme`] (`.theme(&t)`); without one, a button uses
//! [`Theme::normal`] for the current tokens, and `.color(c)` is short for
//! `.theme(&Theme::colored(.., c))`.

use std::sync::Arc;

use egui::{
    Color32, CursorIcon, FontId, Galley, Pos2, Rect, Response, Sense, TextStyle, Ui, Vec2, Widget,
    WidgetInfo, WidgetText, WidgetType, vec2,
};

use crate::theme::{self, Tokens};

/// How a [`Button`] or [`Selectable`] looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// Text at rest; the fill when hovered or selected.
    pub color: Color32,
    /// The dim resting fill; the text when hovered or selected.
    pub fill: Color32,
    /// Text when disabled.
    pub disabled_text: Color32,
    pub rounding: f32,
    /// Around the text. `small` buttons drop the vertical part.
    pub padding: Vec2,
    /// Smallest size, whatever the text (`small` buttons ignore the height).
    pub min_size: Vec2,
}

impl Theme {
    /// The usual button: in the selectable colour (cyan).
    pub fn normal(t: &Tokens) -> Self {
        Self::colored(t, t.cyan())
    }

    /// A button in `color`, with the fill the tokens give that colour.
    pub fn colored(t: &Tokens, color: Color32) -> Self {
        Self {
            color,
            fill: t.button_fill(color),
            disabled_text: t.dim(),
            rounding: t.button_rounding,
            padding: t.spacing.button_padding,
            min_size: vec2(0.0, t.spacing.interact_height),
        }
    }

    /// The same look with a different smallest size.
    pub fn min_size(self, min_size: Vec2) -> Self {
        Self { min_size, ..self }
    }
}

/// Text, theme and size shared by [`Button`] and [`Selectable`].
struct Look {
    text: String,
    theme: Option<Theme>,
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
            theme: None,
            font: None,
            strong: false,
            small: false,
            min_size: Vec2::ZERO,
            centred: false,
        }
    }

    fn theme(&self) -> Theme {
        self.theme
            .unwrap_or_else(|| Theme::normal(&theme::tokens()))
    }

    /// Lays out the text and allocates the widget. With `fill_width` the item takes the
    /// full width of a justified layout (e.g. a popup list).
    fn allocate(&self, ui: &mut Ui, t: &Theme, fill_width: bool) -> (Rect, Response, Arc<Galley>) {
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

        let mut pad = t.padding;
        let mut min = t.min_size;
        if self.small {
            pad.y = 0.0;
            min.y = 0.0;
        }
        let size = (galley.size() + 2.0 * pad).max(min).max(self.min_size);
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
        t: &Theme,
        centred: bool,
    ) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        let p = ui.painter();
        if let Some(fill) = fill {
            p.rect_filled(rect, t.rounding, fill);
        }
        let y = rect.center().y - galley.size().y / 2.0;
        let x = if centred {
            rect.center().x - galley.size().x / 2.0
        } else {
            rect.left() + t.padding.x
        };
        p.galley(Pos2::new(x, y), galley, text);
    }
}

macro_rules! look_builders {
    () => {
        /// The look (default: [`Theme::normal`]).
        pub fn theme(mut self, t: &Theme) -> Self {
            self.look.theme = Some(*t);
            self
        }
        /// Short for `.theme(&Theme::colored(&theme::tokens(), c))`.
        pub fn color(mut self, c: Color32) -> Self {
            self.look.theme = Some(Theme::colored(&theme::tokens(), c));
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
        /// A smallest size on top of the theme's (for one-off layout needs; a size that
        /// belongs to the look goes in [`Theme::min_size`]).
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
        let t = self.look.theme();
        let (rect, resp, galley) = self.look.allocate(ui, &t, false);
        let (color, base) = (t.color, t.fill);
        let (fill, text) = if !ui.is_enabled() {
            (base.gamma_multiply(0.5), t.disabled_text)
        } else if resp.hovered() || resp.is_pointer_button_down_on() {
            (color, base)
        } else {
            (base, color)
        };
        Look::paint(ui, rect, galley, Some(fill), text, &t, true);
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
        let t = self.look.theme();
        let (rect, resp, galley) = self.look.allocate(ui, &t, true);
        let (color, base) = (t.color, t.fill);
        let hovered = resp.hovered() || resp.is_pointer_button_down_on();
        let (fill, text) = match (ui.is_enabled(), self.selected) {
            (false, true) => (Some(base), t.disabled_text),
            (false, false) => (None, t.disabled_text),
            (true, true) => (Some(color), base),
            (true, false) if hovered => (Some(base), color),
            (true, false) => (None, color),
        };
        Look::paint(ui, rect, galley, fill, text, &t, self.look.centred);
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
