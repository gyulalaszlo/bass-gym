//! The Scale menu in Basic scales' toolbar: which scale, its pattern and the neck. A
//! tier-1 view.

use bg_core::harmony::Scale;
use bg_ui::egui::{RichText, Ui};
use bg_ui::kit::{Emit, View, edited};
use bg_ui::theme::FormTheme;
use bg_ui::views::neck::Tuning;
use bg_ui::widgets::button::selectable_value;
use bg_ui::widgets::controls::number_field;

use super::{BUILT_IN, CustomScale, ScaleSettings};

pub struct ScaleMenu;

pub struct Props<'a> {
    pub settings: &'a ScaleSettings,
    pub key: &'a Scale,
    pub library: &'a [CustomScale],
}

impl View for ScaleMenu {
    type Props<'a> = Props<'a>;
    type Theme = FormTheme;
    type Emission = ScaleSettings;

    fn view(p: &Props, theme: &FormTheme, ui: &mut Ui, out: &mut Emit<ScaleSettings>) {
        let note = |text: &str| RichText::new(text).small().color(theme.note);
        let edit = edited(p.settings, |s| {
            ui.horizontal_wrapped(|ui| {
                let key = format!("{} (key)", p.key.mode.label());
                selectable_value(ui, &mut s.scale, None, key);
                for (name, _) in BUILT_IN {
                    selectable_value(ui, &mut s.scale, Some(name.to_owned()), name);
                }
                for c in p.library {
                    selectable_value(ui, &mut s.scale, Some(c.name.clone()), &c.name);
                }
            });
            ui.label(note("Custom scales: Preferences › Basic scales."));
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
