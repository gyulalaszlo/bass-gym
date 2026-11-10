//! The progression strip: one cell per chord, name above numeral, the sounding one
//! highlighted. A tier-1 view; one row that scrolls sideways if it doesn't fit.

use std::convert::Infallible;

use bg_ui::egui::{self, Color32, FontId, Sense, Stroke, StrokeKind, Ui, vec2};
use bg_ui::kit::{Emit, View};
use bg_ui::theme::Tokens;

pub struct Strip;

#[derive(Clone, Debug, PartialEq)]
pub struct StripChord {
    pub label: String,
    /// Empty when chord names are hidden.
    pub roman: String,
}

pub struct Props<'a> {
    pub chords: &'a [StripChord],
    /// The chord sounding now.
    pub current: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name_font: FontId,
    pub roman_font: FontId,
    pub name: Color32,
    pub active_name: Color32,
    pub roman: Color32,
    pub frame: Stroke,
    pub active_frame: Stroke,
    pub active_fill: Color32,
    pub rounding: f32,
    /// Around a cell's text, and between cells.
    pub margin: f32,
    /// A cell's height: room for both lines at the text size.
    pub cell_height: f32,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        let (name, roman) = (32.0, 24.0);
        let margin = t.em(12.0);
        Self {
            name_font: t.prop(name),
            roman_font: t.prop(roman),
            name: t.cyan(),
            active_name: t.text(),
            roman: t.dim(),
            frame: Stroke::new(1.0, t.grid()),
            active_frame: Stroke::new(1.5, t.cyan()),
            active_fill: t.cyan().gamma_multiply(0.18),
            rounding: 2.0,
            margin,
            cell_height: t.em(name + roman + 16.0) + 2.0 * margin,
        }
    }
}

impl View for Strip {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(p: &Props, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        egui::ScrollArea::horizontal()
            .id_salt("bc_strip")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.margin;
                    for (i, c) in p.chords.iter().enumerate() {
                        cell(ui, theme, c, p.current == Some(i));
                    }
                });
            });
    }
}

fn cell(ui: &mut Ui, theme: &Theme, c: &StripChord, active: bool) {
    let name_color = if active {
        theme.active_name
    } else {
        theme.name
    };
    let painter = ui.painter();
    let name = painter.layout_no_wrap(c.label.clone(), theme.name_font.clone(), name_color);
    let roman = painter.layout_no_wrap(c.roman.clone(), theme.roman_font.clone(), theme.roman);
    let size = vec2(
        name.size().x.max(roman.size().x) + theme.margin * 3.0,
        theme.cell_height,
    );
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    if active {
        p.rect_filled(rect, theme.rounding, theme.active_fill);
        p.rect_stroke(rect, theme.rounding, theme.active_frame, StrokeKind::Inside);
    } else {
        p.rect_stroke(rect, theme.rounding, theme.frame, StrokeKind::Inside);
    }
    let name_at = egui::pos2(
        rect.center().x - name.size().x / 2.0,
        rect.top() + theme.margin,
    );
    let roman_at = egui::pos2(
        rect.center().x - roman.size().x / 2.0,
        rect.bottom() - theme.margin - roman.size().y,
    );
    p.galley(name_at, name, name_color);
    p.galley(roman_at, roman, theme.roman);
}
