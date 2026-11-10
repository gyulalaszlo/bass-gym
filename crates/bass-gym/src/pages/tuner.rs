//! The Tuner page: reference pitch and how close counts as in tune.

use bg_ui::egui::{RichText, Ui};
use bg_ui::kit::{self, Emit, ThemeCache, View};
use bg_ui::widgets::controls::{number_field, slider_field};
use bg_ui::widgets::tuner::TunerPrefs;
use bg_ui::{PreferencePage, PrefsCtx, icons};

use super::{PageTheme, grid};

#[derive(Default)]
pub struct TunerPage {
    theme: ThemeCache<PageTheme>,
}

struct Tuner;

impl View for Tuner {
    type Props<'a> = TunerPrefs;
    type Theme = PageTheme;
    type Emission = TunerPrefs;

    fn view(p: &TunerPrefs, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<TunerPrefs>) {
        let edit = kit::edited(p, |p| {
            grid("tuner_prefs", theme).show(ui, |ui| {
                ui.label("Reference A4");
                ui.add(
                    number_field(&mut p.a4)
                        .range(415.0..=466.0)
                        .step(0.5)
                        .suffix("Hz"),
                );
                ui.end_row();
                ui.label(RichText::new("In tune (green) within").color(theme.ok));
                ui.add(slider_field(&mut p.in_tune_cents, 0.5..=10.0).suffix("¢"));
                ui.end_row();
                ui.label(RichText::new("Close (amber) within").color(theme.warning));
                ui.add(slider_field(&mut p.close_cents, p.in_tune_cents..=30.0).suffix("¢"));
                ui.end_row();
            });
        });
        if let Some(p) = edit {
            out.send(p);
        }
    }
}

impl PreferencePage for TunerPage {
    fn id(&self) -> &'static str {
        TunerPrefs::ID
    }
    fn title(&self) -> &'static str {
        "Tuner"
    }
    fn icon(&self) -> &'static str {
        icons::GUITAR
    }

    fn ui(&mut self, ui: &mut Ui, ctx: &mut PrefsCtx) {
        let theme = self.theme.get();
        let prefs: TunerPrefs = ctx.settings.get(TunerPrefs::ID);
        for p in kit::show::<Tuner>(ui, "tuner", &prefs, &theme) {
            ctx.settings.set(TunerPrefs::ID, &p);
        }
    }
}
