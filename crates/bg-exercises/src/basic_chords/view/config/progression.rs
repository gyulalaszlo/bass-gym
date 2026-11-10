//! The Progression section: which chords, how long each lasts, and the seed. A tier-1
//! view.

use bg_core::harmony::{self, Scale};
use bg_ui::egui::{self, RichText, Ui};
use bg_ui::icons;
use bg_ui::kit::{Emit, View, edited, field};
use bg_ui::theme::FormTheme;
use bg_ui::widgets::button::{button, selectable_value};
use bg_ui::widgets::controls::{checkbox, dropdown, number_field, text_field};

use crate::basic_chords::data::{ChordLength, FixedSource, ProgressionKind, ProgressionSettings};

pub struct Progression;

pub struct Props<'a> {
    pub progression: &'a ProgressionSettings,
    pub chord_length: ChordLength,
    pub seed: u64,
    pub new_seed_each_start: bool,
    /// The key, to check typed numerals against.
    pub key: &'a Scale,
    /// Seed of the last started session, if any.
    pub run_seed: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Emission {
    Progression(ProgressionSettings),
    ChordLength(ChordLength),
    Seed(u64),
    NewSeedEachStart(bool),
    /// Use the last session's seed from now on.
    KeepRunSeed(u64),
}

/// The seed after `seed`, for the "New" button.
pub fn next_seed(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
        >> 16
}

/// `pool` with library progression `id` switched `on`. An empty pool means all of the
/// library, and so does one that names it all.
pub fn toggle_pool(pool: &[String], id: &str, on: bool) -> Vec<String> {
    let mut pool = if pool.is_empty() {
        harmony::LIBRARY.iter().map(|e| e.0.to_owned()).collect()
    } else {
        pool.to_vec()
    };
    if on {
        pool.push(id.to_owned());
    } else {
        pool.retain(|x| x != id);
    }
    if pool.len() >= harmony::LIBRARY.len() {
        pool.clear();
    }
    pool
}

impl View for Progression {
    type Props<'a> = Props<'a>;
    type Theme = FormTheme;
    type Emission = Emission;

    fn view(p: &Props, theme: &FormTheme, ui: &mut Ui, out: &mut Emit<Emission>) {
        let pr = p.progression;
        let with = |f: &dyn Fn(&mut ProgressionSettings)| {
            let mut pr = pr.clone();
            f(&mut pr);
            Emission::Progression(pr)
        };
        let kind = edited(&pr.kind, |k| {
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(k, ProgressionKind::Fixed, "Pre-determined");
                ui.radio_value(k, ProgressionKind::SemiRandom, "Semi-random");
                ui.radio_value(k, ProgressionKind::Random, "Random");
            });
        });
        if let Some(k) = kind {
            out.send(with(&|pr| pr.kind = k));
        }
        ui.add_space(theme.gap);
        match pr.kind {
            ProgressionKind::Fixed => fixed(p, theme, ui, out),
            ProgressionKind::SemiRandom => {
                label(ui, theme, "A new pick on every start, from:");
                for (id, name, _) in harmony::LIBRARY {
                    let on = pr.pool.is_empty() || pr.pool.iter().any(|x| x == id);
                    let toggled = |on| with(&|pr| pr.pool = toggle_pool(&pr.pool, id, on));
                    field(ui, &on, out, toggled, |ui, v| checkbox(ui, v, name));
                }
            }
            ProgressionKind::Random => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Length");
                    let len = |v| with(&|pr| pr.random_len = v);
                    field(ui, &pr.random_len, out, len, |ui, v| {
                        ui.add(number_field(v).range(2..=16).suffix("chords"))
                    });
                });
                label(ui, theme, "In key; neighbouring chords share notes.");
            }
        }
        ui.add_space(theme.gap);
        ui.horizontal_wrapped(|ui| {
            ui.label("Each chord");
            let length = edited(&p.chord_length, |len| {
                dropdown(ui, "bc_len", p.chord_length.label(), |ui| {
                    for l in ChordLength::ALL {
                        selectable_value(ui, len, l, l.label());
                    }
                });
            });
            if let Some(l) = length {
                out.send(Emission::ChordLength(l));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Seed");
            field(ui, &p.seed, out, Emission::Seed, |ui, v| {
                ui.add(number_field(v))
            });
            if ui.add(button(format!("{} New", icons::SHUFFLE))).clicked() {
                out.send(Emission::Seed(next_seed(p.seed)));
            }
            field(
                ui,
                &p.new_seed_each_start,
                out,
                Emission::NewSeedEachStart,
                |ui, v| checkbox(ui, v, "New each start"),
            );
        });
        if let Some(s) = p.run_seed.filter(|&s| s != p.seed)
            && ui
                .add(button(format!(
                    "{} Keep the last session's chords",
                    icons::PUSH_PIN
                )))
                .on_hover_text("Use its seed")
                .clicked()
        {
            out.send(Emission::KeepRunSeed(s));
        }
    }
}

/// A library progression or a typed one.
fn fixed(p: &Props, theme: &FormTheme, ui: &mut Ui, out: &mut Emit<Emission>) {
    let pr = p.progression;
    let with = |f: &dyn Fn(&mut ProgressionSettings)| {
        let mut pr = pr.clone();
        f(&mut pr);
        Emission::Progression(pr)
    };
    let source = edited(&pr.source, |s| {
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(s, FixedSource::Library, "Common");
            ui.radio_value(s, FixedSource::Custom, "Custom");
        });
    });
    if let Some(s) = source {
        out.send(with(&|pr| pr.source = s));
    }
    match pr.source {
        FixedSource::Library => {
            let name = harmony::library(&pr.library).map_or("?", |e| e.1);
            let library = edited(&pr.library, |lib| {
                dropdown(ui, "bc_library", name, |ui| {
                    for (id, name, text) in harmony::LIBRARY {
                        selectable_value(ui, lib, id.to_owned(), name).on_hover_text(text);
                    }
                });
            });
            if let Some(l) = library {
                out.send(with(&|pr| pr.library = l.clone()));
            }
        }
        FixedSource::Custom => {
            let custom = |v: String| with(&|pr| pr.custom = v.clone());
            field(ui, &pr.custom, out, custom, |ui, v| {
                ui.add(
                    text_field(v)
                        .hint("e.g. ii7 V7 I7 vi7")
                        .width(ui.available_width().min(theme.field_width))
                        .font(theme.value_font.clone()),
                )
            });
            match harmony::parse_progression(&pr.custom, p.key) {
                Ok(specs) => note(
                    ui,
                    theme,
                    &format!(
                        "{} chords · repeat a numeral to hold it longer",
                        specs.len()
                    ),
                ),
                Err(e) => {
                    ui.add(
                        egui::Label::new(RichText::new(e.message).small().color(theme.error))
                            .wrap(),
                    );
                }
            }
        }
    }
}

/// A one-line note.
fn label(ui: &mut Ui, theme: &FormTheme, text: &str) {
    ui.label(RichText::new(text).small().color(theme.note));
}

/// A note that wraps.
fn note(ui: &mut Ui, theme: &FormTheme, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(theme.note)).wrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_toggles_and_empty_means_all() {
        let all = harmony::LIBRARY.len();
        let first = harmony::LIBRARY[0].0;
        let off = toggle_pool(&[], first, false);
        assert_eq!(off.len(), all - 1);
        assert!(!off.iter().any(|x| x == first));
        assert!(toggle_pool(&off, first, true).is_empty());
    }
}
