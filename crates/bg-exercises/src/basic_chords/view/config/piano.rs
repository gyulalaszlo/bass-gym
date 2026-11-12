//! The Piano menu: how the simulated player comps, how it sounds, and the keyboard
//! display, as three captioned blocks. A tier-1 view.

use bg_core::harmony::{self, comp};
use bg_ui::egui::{self, RichText, Ui};
use bg_ui::kit::{Emit, View, edited, field};
use bg_ui::theme::{FormTheme, caption};
use bg_ui::views::keyboard::{self, KeyLabels};
use bg_ui::widgets::button::selectable_value;
use bg_ui::widgets::controls::{checkbox, dropdown, dropdown_value, slider_field};

use crate::basic_chords::data::KeyboardOptions;

pub struct Piano;

pub struct Props<'a> {
    /// Comp pattern id.
    pub comp: &'a String,
    pub jazziness: f32,
    pub width: f32,
    pub center_octave: i8,
    pub humanize: f32,
    pub piano_volume: f32,
    pub release: f32,
    pub keyboard: &'a KeyboardOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Emission {
    Comp(String),
    Jazziness(f32),
    Width(f32),
    CenterOctave(i8),
    Humanize(f32),
    PianoVolume(f32),
    Release(f32),
    Keyboard(KeyboardOptions),
}

const JAZZ_LEVELS: [&str; 4] = [
    "Triads in close position, root in the left hand",
    "Seventh chords with smooth inversions",
    "Ninths, open (drop-2) voicings, left-hand shells",
    "Rich colour (9/11/13), rootless voicings, anticipations",
];

fn jazz_description(j: f32) -> &'static str {
    JAZZ_LEVELS[((j * 4.0) as usize).min(3)]
}

fn width_description(w: f32) -> &'static str {
    match w {
        w if w < 0.05 => "One hand, close voicings",
        w if w < 0.95 => "Mostly one hand, sometimes two",
        w if w < 1.05 => "Two hands: bass and fifth below, chord above",
        w if w < 1.95 => "Two hands, sometimes doubled in octaves",
        _ => "Two hands, doubled in octaves: big and full",
    }
}

/// A 0..1 setting shown as a percentage, sending `msg` with the 0..1 value.
fn percent(
    ui: &mut Ui,
    value: f32,
    max: f32,
    step: f64,
    out: &mut Emit<Emission>,
    msg: fn(f32) -> Emission,
) -> egui::Response {
    field(
        ui,
        &(value * 100.0),
        out,
        |pct| msg(pct / 100.0),
        |ui, v| ui.add(slider_field(v, 0.0..=max).step_by(step).suffix("%")),
    )
}

impl View for Piano {
    type Props<'a> = Props<'a>;
    type Theme = FormTheme;
    type Emission = Emission;

    fn view(p: &Props, theme: &FormTheme, ui: &mut Ui, out: &mut Emit<Emission>) {
        let note = |text: &str| RichText::new(text).small().color(theme.note);
        caption(ui, "Comping");
        egui::Grid::new("bc_piano_comp")
            .num_columns(2)
            .spacing(theme.grid_spacing)
            .show(ui, |ui| {
                ui.label("Comping");
                let name = comp::PATTERNS
                    .iter()
                    .find(|c| c.0 == *p.comp)
                    .map_or("?", |c| c.1);
                let picked = edited(p.comp, |c| {
                    dropdown(ui, "bc_comp", name, |ui| {
                        for (id, name) in comp::PATTERNS {
                            selectable_value(ui, c, id.to_owned(), name);
                        }
                    });
                });
                if let Some(c) = picked {
                    out.send(Emission::Comp(c));
                }
                ui.end_row();

                ui.label("Jazziness");
                percent(ui, p.jazziness, 100.0, 5.0, out, Emission::Jazziness);
                ui.end_row();
                ui.label("");
                let text = note(jazz_description(p.jazziness));
                ui.add(egui::Label::new(text).wrap());
                ui.end_row();

                ui.label("Width");
                percent(ui, p.width, 200.0, 10.0, out, Emission::Width).on_hover_text(
                    "0% one hand · 100% two hands · 200% two hands with octave doublings; in between mixes them",
                );
                ui.end_row();
                ui.label("");
                let text = note(width_description(p.width));
                ui.label(text);
                ui.end_row();

                ui.label("Register");
                field(ui, &p.center_octave, out, Emission::CenterOctave, |ui, v| {
                    dropdown_value(ui, "bc_octave", v, 2..=6i8, |o| {
                        match o - harmony::voicing::CENTER_OCTAVE {
                            0 => "Middle (default)".to_owned(),
                            n => format!("{n:+} octave{}", if n.abs() == 1 { "" } else { "s" }),
                        }
                    })
                });
                ui.end_row();

                ui.label("Humanize");
                percent(ui, p.humanize, 100.0, 5.0, out, Emission::Humanize)
                    .on_hover_text("Small random changes in timing, chord rolls and dynamics");
                ui.end_row();
            });

        ui.add_space(theme.group_gap);
        caption(ui, "Sound");
        egui::Grid::new("bc_piano_sound")
            .num_columns(2)
            .spacing(theme.grid_spacing)
            .show(ui, |ui| {
                ui.label("Volume");
                field(ui, &p.piano_volume, out, Emission::PianoVolume, |ui, v| {
                    ui.add(egui::Slider::new(v, 0.0..=1.0).show_value(false))
                });
                ui.end_row();

                ui.label("Release");
                field(ui, &p.release, out, Emission::Release, |ui, v| {
                    ui.add(
                        slider_field(v, 0.015..=3.0)
                            .logarithmic()
                            .decimals(2)
                            .suffix("s"),
                    )
                })
                .on_hover_text(
                    "Lets sampled piano notes ring on after they end, until the next chord \
                     attack, so the harmony stays in the air at slow tempos",
                );
                ui.end_row();
            });
        ui.add_space(theme.group_gap);
        caption(ui, "Display");
        let k = p.keyboard;
        let keyboard = edited(k, |k| {
            checkbox(ui, &mut k.visible, "Show the keyboard");
            ui.add_enabled_ui(k.visible, |ui| {
                egui::Grid::new("bc_piano_keyboard")
                    .num_columns(2)
                    .spacing(theme.grid_spacing)
                    .show(ui, |ui| {
                        ui.label("Keyboard");
                        dropdown(ui, "bc_keys", format!("{} keys", k.keys), |ui| {
                            for (n, _, _) in keyboard::SIZES {
                                selectable_value(ui, &mut k.keys, n, format!("{n} keys"));
                            }
                        });
                        ui.end_row();
                        ui.label("Key labels");
                        dropdown(ui, "bc_labels", k.labels.label(), |ui| {
                            for l in KeyLabels::ALL {
                                selectable_value(ui, &mut k.labels, l, l.label());
                            }
                        });
                        ui.end_row();
                    });
                checkbox(ui, &mut k.ghost_next, "Outline the next chord");
                checkbox(ui, &mut k.show_bass, "Show my bass note");
            });
            // Also hides them in the strip and the status, so it works without the
            // keyboard too.
            checkbox(
                ui,
                &mut k.hide_chord_names,
                "Hide chord names (read the keyboard)",
            );
        });
        if let Some(k) = keyboard {
            out.send(Emission::Keyboard(k));
        }
    }
}
