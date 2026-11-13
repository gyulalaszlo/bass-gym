//! The Scale menu in Basic scales' toolbar: which scale (picked into the preset, where
//! its degrees can be changed for this preset alone), its pattern and the neck. A tier-1
//! view.

use bg_core::harmony::Scale;
use bg_ui::egui::{RichText, Ui};
use bg_ui::kit::{Emit, View, edited};
use bg_ui::theme::FormTheme;
use bg_ui::views::neck::Tuning;
use bg_ui::widgets::button::{selectable, selectable_value};
use bg_ui::widgets::controls::{checkbox, number_field, text_field};

use super::prefs::{DEGREES, toggle};
use super::{BUILT_IN, ScaleDef, ScaleSettings, mode_intervals};

pub struct ScaleMenu;

pub struct Props<'a> {
    pub settings: &'a ScaleSettings,
    pub key: &'a Scale,
    pub library: &'a [ScaleDef],
}

impl View for ScaleMenu {
    type Props<'a> = Props<'a>;
    type Theme = FormTheme;
    type Emission = ScaleSettings;

    fn view(p: &Props, theme: &FormTheme, ui: &mut Ui, out: &mut Emit<ScaleSettings>) {
        let note = |text: &str| RichText::new(text).small().color(theme.note);
        let edit = edited(p.settings, |s| {
            let picked = s.scale.as_ref().map(|d| d.name.clone());
            ui.horizontal_wrapped(|ui| {
                let key = format!("{} (key)", p.key.mode.label());
                if ui.add(selectable(picked.is_none(), key)).clicked() {
                    s.scale = None;
                }
                let built_in = BUILT_IN.iter().map(|&(name, iv)| ScaleDef {
                    name: name.to_owned(),
                    intervals: iv.to_vec(),
                });
                for d in built_in.chain(p.library.iter().cloned()) {
                    let on = picked.as_deref() == Some(d.name.as_str());
                    if ui.add(selectable(on, &d.name)).clicked() {
                        s.scale = Some(d);
                    }
                }
            });
            ui.label(note("More in Preferences › Basic scales."));
            ui.add_space(theme.gap);
            // This preset's own copy: changing it changes no other preset.
            let intervals = s.intervals(p.key);
            ui.horizontal_wrapped(|ui| {
                for (semi, degree) in (0u8..).zip(DEGREES) {
                    let on = intervals.contains(&semi);
                    let resp = ui.add_enabled(semi != 0, selectable(on, degree));
                    if resp.clicked() {
                        let name = s.name(p.key);
                        let def = s.scale.get_or_insert_with(|| ScaleDef {
                            name,
                            intervals: mode_intervals(p.key),
                        });
                        def.intervals = toggle(&intervals, semi, !on);
                    }
                }
            });
            if let Some(def) = s.scale.as_mut() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Name");
                    ui.add(text_field(&mut def.name).width(theme.field_width.min(200.0)));
                });
            }
            ui.add_space(theme.group_gap);
            ui.horizontal_wrapped(|ui| {
                ui.label("Span");
                ui.add(number_field(&mut s.octaves).range(1..=3).suffix("oct"));
                ui.label("from");
                ui.add(number_field(&mut s.start).range(0..=2).suffix("oct up"));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Every");
                ui.add(number_field(&mut s.step).range(1..=7).suffix("th note"));
            });
            ui.label(note(
                "1 plays every note, 2 every other (a scale in thirds), ...",
            ));
            checkbox(ui, &mut s.whole_bars, "Root to root, in whole bars");
            ui.label(note(
                "Ends on the top root and back on the bottom one, then repeats the root until the bar is full, so every round starts on a bar line.",
            ));
            ui.add_space(theme.group_gap);
            ui.horizontal_wrapped(|ui| {
                ui.label("Neck");
                for t in Tuning::ALL {
                    selectable_value(ui, &mut s.tuning, t, t.label());
                }
            });
        });
        if let Some(s) = edit {
            out.send(s);
        }
    }
}
