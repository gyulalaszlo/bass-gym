//! The live status in the toolbar: the chord now (or the count-in) and the next one.
//! A tier-1 view.

use std::convert::Infallible;

use bg_ui::egui::{Color32, FontId, RichText, Ui};
use bg_ui::kit::{Emit, View};
use bg_ui::theme::Tokens;

pub struct StatusLine;

#[derive(Clone, Debug, PartialEq)]
pub struct ChordStatus {
    /// The chord sounding now and its numeral (unless chord names are hidden); none
    /// during the count-in.
    pub current: Option<(String, Option<String>)>,
    pub next: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub chord_font: FontId,
    pub chord: Color32,
    pub roman_font: FontId,
    pub roman: Color32,
    pub count_in_font: FontId,
    pub count_in: Color32,
    pub next_label_font: FontId,
    pub next_label: Color32,
    pub next_font: FontId,
    pub next: Color32,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            chord_font: t.mono(26.0),
            chord: t.cyan(),
            roman_font: t.mono(16.0),
            roman: t.dim(),
            count_in_font: t.mono(18.0),
            count_in: t.amber(),
            next_label_font: t.mono(13.0),
            next_label: t.text(),
            next_font: t.mono(16.0),
            next: t.magenta(),
        }
    }
}

impl View for StatusLine {
    type Props<'a> = ChordStatus;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(s: &ChordStatus, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        // Labels go straight into the parent's row (the toolbar lays out left to right).
        let text = |t: &str, font: &FontId, c| RichText::new(t).font(font.clone()).color(c);
        match &s.current {
            Some((chord, roman)) => {
                ui.label(text(chord, &theme.chord_font, theme.chord));
                if let Some(r) = roman {
                    ui.label(text(r, &theme.roman_font, theme.roman));
                }
            }
            None => {
                ui.label(text("COUNT-IN", &theme.count_in_font, theme.count_in));
            }
        }
        if let Some(n) = &s.next {
            ui.label(text("NEXT", &theme.next_label_font, theme.next_label));
            ui.label(text(n, &theme.next_font, theme.next));
        }
    }
}
