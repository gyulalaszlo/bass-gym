//! Visual language inspired by Airbus ECAM / flight-computer displays: a black
//! background, thin light structure lines, large monospaced text, and colour reserved
//! for meaning (green = normal values, cyan = selectable / units, amber = caution,
//! red = fault, magenta = targets).
//!
//! The palette is runtime-configurable ([`ThemePrefs`]). There are two layers:
//!
//! - [`Tokens`]: the global design tokens (palette, text scale, button look), read with
//!   [`tokens`]. Component `Theme` constructors read these, and painters don't.
//! - A `Theme` struct per component, built from `&Tokens` and handed down by its parent
//!   (see `docs/architecture/ui-components.md`).
//!
//! Code that isn't migrated yet still reads the free accessors (`theme::green()`, ...),
//! which read the same globals, so a theme change still applies everywhere.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
use serde::{Deserialize, Serialize};

use bg_core::Grade;

use crate::widgets::score::MUSIC_FONT;

/// An sRGB colour, stored as `[r, g, b]` in the settings file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    pub const fn hex(v: u32) -> Self {
        Self([(v >> 16) as u8, (v >> 8) as u8, v as u8])
    }

    pub const fn color(self) -> Color32 {
        Color32::from_rgb(self.0[0], self.0[1], self.0[2])
    }

    fn luminance(self) -> f32 {
        (0.2126 * self.0[0] as f32 + 0.7152 * self.0[1] as f32 + 0.0722 * self.0[2] as f32) / 255.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Palette {
    /// Display background.
    pub bg: Rgb,
    /// Panels and windows.
    pub panel: Rgb,
    /// Slightly raised areas (count-in bars, hover backgrounds).
    pub raised: Rgb,
    /// Structure lines: frames, separators, bar lines.
    pub line: Rgb,
    /// Faint grid lines.
    pub grid: Rgb,
    /// Secondary text and captions.
    pub dim: Rgb,
    /// Primary text and labels.
    pub text: Rgb,
    pub green: Rgb,
    pub amber: Rgb,
    pub red: Rgb,
    pub cyan: Rgb,
    pub magenta: Rgb,
    /// Base of the button fill: blended with each button's colour (see
    /// [`ThemePrefs::button_tint`]) for the dim resting fill.
    pub button_base: Rgb,
}

impl Default for Palette {
    fn default() -> Self {
        ECAM
    }
}

pub const ECAM: Palette = Palette {
    bg: Rgb::hex(0x000000),
    panel: Rgb::hex(0x000000),
    raised: Rgb::hex(0x10151a),
    line: Rgb::hex(0xb4bac0),
    grid: Rgb::hex(0x394047),
    dim: Rgb::hex(0xa2abb4),
    text: Rgb::hex(0xffffff),
    green: Rgb::hex(0x1ee35c),
    amber: Rgb::hex(0xffa21a),
    red: Rgb::hex(0xff3434),
    cyan: Rgb::hex(0x16e0ff),
    magenta: Rgb::hex(0xff45ee),
    button_base: Rgb::hex(0x000000),
};

pub const ECAM_NIGHT: Palette = Palette {
    bg: Rgb::hex(0x000000),
    panel: Rgb::hex(0x000000),
    raised: Rgb::hex(0x0a0d10),
    line: Rgb::hex(0x6f757b),
    grid: Rgb::hex(0x24292e),
    dim: Rgb::hex(0x6e767e),
    text: Rgb::hex(0xb8bcc0),
    green: Rgb::hex(0x13a843),
    amber: Rgb::hex(0xbd7812),
    red: Rgb::hex(0xc22828),
    cyan: Rgb::hex(0x10a5bd),
    magenta: Rgb::hex(0xbd33b0),
    button_base: Rgb::hex(0x000000),
};

pub const HIGH_CONTRAST: Palette = Palette {
    bg: Rgb::hex(0x000000),
    panel: Rgb::hex(0x000000),
    raised: Rgb::hex(0x1a1a1a),
    line: Rgb::hex(0xffffff),
    grid: Rgb::hex(0x5a5a5a),
    dim: Rgb::hex(0xd0d0d0),
    text: Rgb::hex(0xffffff),
    green: Rgb::hex(0x00ff40),
    amber: Rgb::hex(0xffc000),
    red: Rgb::hex(0xff2020),
    cyan: Rgb::hex(0x00ffff),
    magenta: Rgb::hex(0xff00ff),
    button_base: Rgb::hex(0x000000),
};

pub const SLATE: Palette = Palette {
    bg: Rgb::hex(0x0a0d11),
    panel: Rgb::hex(0x10151b),
    raised: Rgb::hex(0x181f27),
    line: Rgb::hex(0x56616d),
    grid: Rgb::hex(0x2c343d),
    dim: Rgb::hex(0x8b96a2),
    text: Rgb::hex(0xe8ecef),
    green: Rgb::hex(0x2ee56b),
    amber: Rgb::hex(0xffb01f),
    red: Rgb::hex(0xff453a),
    cyan: Rgb::hex(0x2bd4f5),
    magenta: Rgb::hex(0xf05ce0),
    button_base: Rgb::hex(0x0a0d11),
};

pub const DAY: Palette = Palette {
    bg: Rgb::hex(0xf4f4f0),
    panel: Rgb::hex(0xf4f4f0),
    raised: Rgb::hex(0xe2e3de),
    line: Rgb::hex(0x4a5056),
    grid: Rgb::hex(0xc4c8cc),
    dim: Rgb::hex(0x56606a),
    text: Rgb::hex(0x0c0e10),
    green: Rgb::hex(0x00873c),
    amber: Rgb::hex(0xb35f00),
    red: Rgb::hex(0xc62828),
    cyan: Rgb::hex(0x006e9a),
    magenta: Rgb::hex(0xa3168f),
    button_base: Rgb::hex(0xf4f4f0),
};

/// Built-in themes: (id, name, palette).
pub const PRESETS: [(&str, &str, Palette); 5] = [
    ("ecam", "ECAM", ECAM),
    ("ecam_night", "ECAM night", ECAM_NIGHT),
    ("high_contrast", "High contrast", HIGH_CONTRAST),
    ("slate", "Slate", SLATE),
    ("day", "Day", DAY),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemePrefs {
    /// A preset id, or `"custom"` to use `custom`.
    pub preset: String,
    pub custom: Palette,
    /// Overall UI zoom (also changed with Cmd/Ctrl +/-/0).
    pub zoom: f32,
    /// Text size multiplier on top of the zoom.
    pub text_scale: f32,
    /// Use the monospaced font for all text (ECAM style).
    pub mono_ui: bool,
    /// Corner radius of buttons, in points.
    pub button_rounding: f32,
    /// How much of a button's colour goes into its resting fill (0 = just
    /// `button_base`, 1 = the full button colour).
    pub button_tint: f32,
}

impl Default for ThemePrefs {
    fn default() -> Self {
        Self {
            preset: "ecam".into(),
            custom: ECAM,
            zoom: 1.0,
            text_scale: 1.0,
            mono_ui: true,
            button_rounding: 4.0,
            button_tint: 0.15,
        }
    }
}

impl ThemePrefs {
    pub const ID: &'static str = "display";

    pub fn palette(&self) -> Palette {
        PRESETS
            .iter()
            .find(|p| p.0 == self.preset)
            .map_or(self.custom, |p| p.2)
    }
}

static PALETTE: RwLock<Palette> = RwLock::new(ECAM);
static TEXT_SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0f32
static BUTTON_ROUNDING: AtomicU32 = AtomicU32::new(0x4080_0000); // 4.0f32
static BUTTON_TINT: AtomicU32 = AtomicU32::new(0x3e19_999a); // 0.15f32

pub fn palette() -> Palette {
    *PALETTE.read().unwrap_or_else(|e| e.into_inner())
}

fn text_scale() -> f32 {
    f32::from_bits(TEXT_SCALE.load(Ordering::Relaxed))
}

pub fn bg() -> Color32 {
    palette().bg.color()
}
pub fn panel() -> Color32 {
    palette().panel.color()
}
pub fn raised() -> Color32 {
    palette().raised.color()
}
pub fn line() -> Color32 {
    palette().line.color()
}
pub fn grid() -> Color32 {
    palette().grid.color()
}
pub fn dim() -> Color32 {
    palette().dim.color()
}
pub fn text() -> Color32 {
    palette().text.color()
}
/// Normal / good / active values.
pub fn green() -> Color32 {
    palette().green.color()
}
/// Caution.
pub fn amber() -> Color32 {
    palette().amber.color()
}
/// Warning / fault.
pub fn red() -> Color32 {
    palette().red.color()
}
/// Selectable / interactive items and units.
pub fn cyan() -> Color32 {
    palette().cyan.color()
}
/// Targets.
pub fn magenta() -> Color32 {
    palette().magenta.color()
}

/// Corner radius of buttons.
pub fn button_rounding() -> f32 {
    f32::from_bits(BUTTON_ROUNDING.load(Ordering::Relaxed))
}

/// The dim resting fill of a button drawn in `color` (also its text colour on hover).
pub fn button_fill(color: Color32) -> Color32 {
    tokens().button_fill(color)
}

pub fn grade_color(g: Grade) -> Color32 {
    tokens().grade_color(g)
}

/// Monospaced font for values, scaled by the text-size preference.
pub fn mono(size: f32) -> FontId {
    tokens().mono(size)
}

/// Text-scaled proportional font (icons live in this family).
pub fn prop(size: f32) -> FontId {
    tokens().prop(size)
}

// ---------------------------------------------------------------------------------------
// Tokens

/// The global design tokens: everything the user's display preferences decide. Component
/// themes are built from these, and rebuilt when they change (see `kit::ThemeCache`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub palette: Palette,
    /// Text size multiplier.
    pub text_scale: f32,
    /// Corner radius of buttons.
    pub button_rounding: f32,
    /// How much of a button's colour goes into its resting fill.
    pub button_tint: f32,
}

impl Default for Tokens {
    fn default() -> Self {
        Self {
            palette: ECAM,
            text_scale: 1.0,
            button_rounding: 4.0,
            button_tint: 0.15,
        }
    }
}

/// The tokens currently applied (see [`apply`]).
pub fn tokens() -> Tokens {
    Tokens {
        palette: palette(),
        text_scale: text_scale(),
        button_rounding: button_rounding(),
        button_tint: f32::from_bits(BUTTON_TINT.load(Ordering::Relaxed)),
    }
}

impl Tokens {
    pub fn bg(&self) -> Color32 {
        self.palette.bg.color()
    }
    pub fn panel(&self) -> Color32 {
        self.palette.panel.color()
    }
    pub fn raised(&self) -> Color32 {
        self.palette.raised.color()
    }
    pub fn line(&self) -> Color32 {
        self.palette.line.color()
    }
    pub fn grid(&self) -> Color32 {
        self.palette.grid.color()
    }
    pub fn dim(&self) -> Color32 {
        self.palette.dim.color()
    }
    pub fn text(&self) -> Color32 {
        self.palette.text.color()
    }
    /// Normal / good / active values.
    pub fn green(&self) -> Color32 {
        self.palette.green.color()
    }
    /// Caution.
    pub fn amber(&self) -> Color32 {
        self.palette.amber.color()
    }
    /// Warning / fault.
    pub fn red(&self) -> Color32 {
        self.palette.red.color()
    }
    /// Selectable / interactive items and units.
    pub fn cyan(&self) -> Color32 {
        self.palette.cyan.color()
    }
    /// Targets.
    pub fn magenta(&self) -> Color32 {
        self.palette.magenta.color()
    }

    /// Monospaced font for values, scaled by the text-size preference.
    pub fn mono(&self, size: f32) -> FontId {
        FontId::new(size * self.text_scale, FontFamily::Monospace)
    }

    /// Text-scaled proportional font (icons live in this family).
    pub fn prop(&self, size: f32) -> FontId {
        FontId::new(size * self.text_scale, FontFamily::Proportional)
    }

    /// The dim resting fill of a button drawn in `color` (also its text colour on hover).
    pub fn button_fill(&self, color: Color32) -> Color32 {
        let t = self.button_tint;
        let base = self.palette.button_base.0;
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color32::from_rgb(
            mix(base[0], color.r()),
            mix(base[1], color.g()),
            mix(base[2], color.b()),
        )
    }

    pub fn grade_color(&self, g: Grade) -> Color32 {
        GradeColors::new(self).get(g)
    }
}

/// The colour of each [`Grade`]: a small theme shared by everything that shows grades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradeColors {
    pub good: Color32,
    pub ok: Color32,
    pub poor: Color32,
    pub missed: Color32,
}

impl GradeColors {
    pub fn new(t: &Tokens) -> Self {
        Self {
            good: t.green(),
            ok: t.amber(),
            poor: t.red(),
            missed: t.red().gamma_multiply(0.5),
        }
    }

    pub fn get(&self, g: Grade) -> Color32 {
        match g {
            Grade::Good => self.good,
            Grade::Ok => self.ok,
            Grade::Poor => self.poor,
            Grade::Missed => self.missed,
        }
    }
}

/// Applies the theme: palette, fonts, text sizes and widget visuals. Call when the
/// preferences change (zoom is handled by the app, see [`ThemePrefs::zoom`]).
pub fn apply(ctx: &egui::Context, prefs: &ThemePrefs) {
    let p = prefs.palette();
    *PALETTE.write().unwrap_or_else(|e| e.into_inner()) = p;
    let scale = prefs.text_scale.clamp(0.6, 2.0);
    TEXT_SCALE.store(scale.to_bits(), Ordering::Relaxed);
    BUTTON_ROUNDING.store(
        prefs.button_rounding.clamp(0.0, 16.0).to_bits(),
        Ordering::Relaxed,
    );
    BUTTON_TINT.store(
        prefs.button_tint.clamp(0.0, 1.0).to_bits(),
        Ordering::Relaxed,
    );

    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    if prefs.mono_ui {
        // Monospace first for all text; the proportional fonts stay as fallbacks (icons).
        let mono_fonts = fonts
            .families
            .get(&FontFamily::Monospace)
            .cloned()
            .unwrap_or_default();
        if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
            for (i, f) in mono_fonts.into_iter().enumerate() {
                prop.retain(|x| *x != f);
                prop.insert(i, f);
            }
        }
    }
    // Icons in monospaced text too (phosphor only registers itself for proportional).
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace)
        && !mono.iter().any(|f| f == "phosphor")
    {
        mono.push("phosphor".to_owned());
    }
    // Notation, in its own family: music glyphs must not mix with text or icons.
    fonts.font_data.insert(
        MUSIC_FONT.to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/Bravura.otf"
        ))),
    );
    fonts.families.insert(
        FontFamily::Name(MUSIC_FONT.into()),
        vec![MUSIC_FONT.to_owned(), "Hack".to_owned()],
    );
    ctx.set_fonts(fonts);

    let dark = p.bg.luminance() < 0.5;
    let c = |rgb: Rgb| rgb.color();
    ctx.global_style_mut(|style| {
        // With `mono_ui` the proportional family resolves to the monospace font first.
        let body = FontFamily::Proportional;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(16.0 * scale, body.clone()));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::new(16.0 * scale, body.clone()));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::new(21.0 * scale, body.clone()));
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(16.0 * scale, FontFamily::Monospace),
        );
        style
            .text_styles
            .insert(TextStyle::Small, FontId::new(13.0 * scale, body));
        style.spacing.item_spacing = egui::vec2(10.0, 7.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 28.0 * scale.max(1.0);

        let v = &mut style.visuals;
        *v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.panel_fill = c(p.panel);
        v.window_fill = c(p.panel);
        v.extreme_bg_color = c(p.bg);
        v.faint_bg_color = c(p.raised);
        v.code_bg_color = c(p.raised);
        v.window_stroke = Stroke::new(1.0, c(p.line));
        v.window_shadow = egui::Shadow::NONE;
        v.popup_shadow = egui::Shadow::NONE;
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.hyperlink_color = c(p.cyan);
        v.warn_fg_color = c(p.amber);
        v.error_fg_color = c(p.red);
        v.selection.bg_fill = c(p.cyan).gamma_multiply(0.22);
        v.selection.stroke = Stroke::new(1.5, c(p.cyan));
        v.text_cursor.stroke = Stroke::new(2.0, c(p.cyan));

        let w = &mut v.widgets;
        for wv in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            wv.corner_radius = CornerRadius::ZERO;
            wv.expansion = 0.0;
        }
        // Static text and frames.
        w.noninteractive.bg_fill = c(p.panel);
        w.noninteractive.weak_bg_fill = c(p.panel);
        w.noninteractive.bg_stroke = Stroke::new(1.0, c(p.grid));
        w.noninteractive.fg_stroke = Stroke::new(1.0, c(p.text));
        // Stroke widths are 0 at rest and a whole 1.0 otherwise: egui sizes a button as
        // `round(padding - stroke) + stroke`, and selectables drop their frame at rest, so
        // any other widths make buttons change size (and the layout wiggle) on hover.
        // Controls: outlined boxes with cyan content, like ECAM selectable fields.
        w.inactive.bg_fill = c(p.raised);
        w.inactive.weak_bg_fill = c(p.bg);
        w.inactive.bg_stroke = Stroke::new(0.0, c(p.line).gamma_multiply(0.6));
        w.inactive.fg_stroke = Stroke::new(1.5, c(p.cyan));
        w.hovered.bg_fill = c(p.raised);
        w.hovered.weak_bg_fill = c(p.raised);
        w.hovered.bg_stroke = Stroke::new(1.0, c(p.cyan));
        w.hovered.fg_stroke = Stroke::new(1.5, c(p.cyan));
        w.active.bg_fill = c(p.cyan).gamma_multiply(0.25);
        w.active.weak_bg_fill = c(p.cyan).gamma_multiply(0.25);
        w.active.bg_stroke = Stroke::new(1.0, c(p.cyan));
        w.active.fg_stroke = Stroke::new(2.0, c(p.text));
        w.open.bg_fill = c(p.raised);
        w.open.weak_bg_fill = c(p.raised);
        w.open.bg_stroke = Stroke::new(1.0, c(p.cyan));
        w.open.fg_stroke = Stroke::new(1.5, c(p.cyan));
    });
}

/// A label caption, ECAM-style (upper case).
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .small()
            .color(dim()),
    );
}

/// A boxed section with an underlined title, like the ECAM page titles. Fills the
/// available width, so sections line up in columns and wrap their content.
pub fn section<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, grid()))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            title_label(ui, title);
            add(ui)
        })
        .inner
}

/// Lays out `count` items in as many equal-width columns as fit (each at least
/// `min_width` wide), row by row; on a narrow display they stack vertically.
pub fn responsive_columns(
    ui: &mut egui::Ui,
    min_width: f32,
    count: usize,
    mut item: impl FnMut(&mut egui::Ui, usize),
) {
    let spacing = ui.spacing().item_spacing.x;
    let cols = (((ui.available_width() + spacing) / (min_width + spacing)).floor() as usize)
        .clamp(1, count.max(1));
    for row in (0..count).step_by(cols) {
        ui.columns(cols, |columns| {
            for (c, col) in columns.iter_mut().enumerate() {
                if row + c < count {
                    item(col, row + c);
                }
            }
        });
    }
}

/// An underlined, upper-case title.
pub fn title_label(ui: &mut egui::Ui, title: &str) {
    let r = ui.label(egui::RichText::new(title.to_uppercase()).color(text()));
    ui.painter().hline(
        r.rect.x_range(),
        r.rect.bottom() + 1.0,
        Stroke::new(1.5, text()),
    );
    ui.add_space(2.0);
}
