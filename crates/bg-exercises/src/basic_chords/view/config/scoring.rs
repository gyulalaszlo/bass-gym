//! The Scoring section: what a session grades. A tier-1 view.

use bg_core::harmony;
use bg_ui::egui::{self, RichText, Ui};
use bg_ui::kit::{Emit, View, edited, field};
use bg_ui::theme::FormTheme;
use bg_ui::widgets::button::selectable_value;
use bg_ui::widgets::controls::{checkbox, dropdown};

use crate::basic_chords::data::ScoringOptions;

pub struct Scoring;

pub struct Props<'a> {
    pub scoring: &'a ScoringOptions,
    /// Chord-compatibility checker id.
    pub checker: &'a String,
}

pub enum Emission {
    Scoring(ScoringOptions),
    Checker(String),
}

impl View for Scoring {
    type Props<'a> = Props<'a>;
    type Theme = FormTheme;
    type Emission = Emission;

    fn view(p: &Props, theme: &FormTheme, ui: &mut Ui, out: &mut Emit<Emission>) {
        let s = p.scoring;
        let with = |f: fn(&mut ScoringOptions, bool)| {
            move |v| {
                let mut s = s.clone();
                f(&mut s, v);
                Emission::Scoring(s)
            }
        };
        ui.label(RichText::new("Targets").small().color(theme.note));
        ui.horizontal_wrapped(|ui| {
            field(ui, &s.groove, out, with(|s, v| s.groove = v), |ui, v| {
                checkbox(ui, v, "Groove")
            })
            .on_hover_text("Play the groove's rhythm");
            field(ui, &s.piano, out, with(|s, v| s.piano = v), |ui, v| {
                checkbox(ui, v, "Piano hits")
            })
            .on_hover_text("Play a note with every piano attack");
        });
        ui.add_space(theme.gap);
        field(
            ui,
            &s.chord_tones,
            out,
            with(|s, v| s.chord_tones = v),
            |ui, v| checkbox(ui, v, "Grade chord fit"),
        );
        ui.add_enabled_ui(s.chord_tones, |ui| {
            let current = harmony::checker(p.checker);
            let picked = edited(p.checker, |checker| {
                dropdown(ui, "bc_checker", current.name(), |ui| {
                    for c in harmony::checkers() {
                        selectable_value(ui, checker, c.id().to_owned(), c.name())
                            .on_hover_text(c.description());
                    }
                });
            });
            if let Some(c) = picked {
                out.send(Emission::Checker(c));
            }
            note(ui, theme, current.description());
        });
        ui.add_space(theme.gap);
        note(ui, theme, s.summary());
    }
}

fn note(ui: &mut Ui, theme: &FormTheme, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(theme.note)).wrap());
}
