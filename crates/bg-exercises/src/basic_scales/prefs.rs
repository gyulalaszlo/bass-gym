//! The user's own scales, and the preferences page that edits them.

use bg_ui::kit::{self, Emit, ThemeCache, View};
use bg_ui::theme::{FormTheme, section};
use bg_ui::widgets::button::{button, selectable};
use bg_ui::widgets::controls::text_field;
use bg_ui::{PreferencePage, PrefsCtx, egui, icons};
use serde::{Deserialize, Serialize};

/// A scale of the user's: its name and semitones above the root (0 first, ascending).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScaleDef {
    pub name: String,
    pub intervals: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScaleLibrary {
    pub scales: Vec<ScaleDef>,
}

impl ScaleLibrary {
    pub const ID: &'static str = "basic_scales";
}

/// Degree names of the semitones above the root.
pub const DEGREES: [&str; 12] = [
    "1", "b2", "2", "b3", "3", "4", "b5", "5", "b6", "6", "b7", "7",
];

/// `intervals` with `semitone` switched `on`, kept ascending. The root stays.
pub fn toggle(intervals: &[u8], semitone: u8, on: bool) -> Vec<u8> {
    (0..12)
        .filter(|&i| {
            i == 0
                || if i == semitone {
                    on
                } else {
                    intervals.contains(&i)
                }
        })
        .collect()
}

/// A name for a new scale that no other has.
pub fn new_name(scales: &[ScaleDef]) -> String {
    (1..)
        .map(|n| format!("My scale {n}"))
        .find(|name| !scales.iter().any(|s| &s.name == name))
        .unwrap_or_default()
}

#[derive(Default)]
pub struct ScalesPage {
    theme: ThemeCache<FormTheme>,
}

struct Editor;

impl View for Editor {
    type Props<'a> = ScaleLibrary;
    type Theme = FormTheme;
    type Emission = ScaleLibrary;

    fn view(
        lib: &ScaleLibrary,
        theme: &FormTheme,
        ui: &mut egui::Ui,
        out: &mut Emit<ScaleLibrary>,
    ) {
        let note = |text: &str| egui::RichText::new(text).small().color(theme.note);
        let mut edit = lib.clone();
        section(ui, "Custom scales", |ui| {
            ui.label(note(
                "Offered in Basic scales' Scale menu, on the key's root. A preset keeps its own copy of the scale it uses, so changes here reach it only when it is picked again.",
            ));
            ui.add_space(theme.gap);
            let mut remove = None;
            for (i, s) in edit.scales.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.add(text_field(&mut s.name).width(theme.field_width.min(180.0)));
                    for (semi, degree) in (0u8..).zip(DEGREES) {
                        let on = s.intervals.contains(&semi);
                        let resp = ui.add_enabled(semi != 0, selectable(on, degree));
                        if resp.clicked() {
                            s.intervals = toggle(&s.intervals, semi, !on);
                        }
                    }
                    if ui
                        .add(button(icons::TRASH))
                        .on_hover_text("Delete")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                edit.scales.remove(i);
            }
            let names: Vec<&str> = edit.scales.iter().map(|s| s.name.trim()).collect();
            let clash = names
                .iter()
                .enumerate()
                .any(|(i, n)| n.is_empty() || names[..i].contains(n));
            if clash {
                ui.colored_label(
                    theme.error,
                    "Every scale needs a name of its own; the first of a name is used.",
                );
            }
            ui.add_space(theme.gap);
            if ui
                .add(button(format!("{} Add scale", icons::PLUS)))
                .clicked()
            {
                edit.scales.push(ScaleDef {
                    name: new_name(&edit.scales),
                    intervals: vec![0, 2, 4, 7, 9],
                });
            }
        });
        if edit != *lib {
            out.send(edit);
        }
    }
}

impl PreferencePage for ScalesPage {
    fn id(&self) -> &'static str {
        ScaleLibrary::ID
    }
    fn title(&self) -> &'static str {
        "Basic scales"
    }
    fn icon(&self) -> &'static str {
        icons::STAIRS
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut PrefsCtx) {
        let theme = self.theme.get();
        let lib: ScaleLibrary = ctx.settings.get(ScaleLibrary::ID);
        for lib in kit::show::<Editor>(ui, "scales", &lib, &theme) {
            ctx.settings.set(ScaleLibrary::ID, &lib);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_keeps_the_root_and_the_order() {
        assert_eq!(toggle(&[0, 4, 7], 2, true), [0, 2, 4, 7]);
        assert_eq!(toggle(&[0, 4, 7], 4, false), [0, 7]);
        assert_eq!(toggle(&[0, 4, 7], 0, false), [0, 4, 7]);
    }

    #[test]
    fn new_scales_get_a_free_name() {
        let s = |n: &str| ScaleDef {
            name: n.into(),
            intervals: vec![0],
        };
        assert_eq!(new_name(&[]), "My scale 1");
        assert_eq!(new_name(&[s("My scale 1"), s("My scale 3")]), "My scale 2");
    }
}
