//! The Scoring page: the timing and length tolerances.

use bg_core::ScoringThresholds;
use bg_ui::egui::{RichText, Ui};
use bg_ui::kit::{self, Emit, ThemeCache, View};
use bg_ui::widgets::controls::slider_field;
use bg_ui::{PreferencePage, PrefsCtx, icons};

use super::{PageTheme, SCORING, grid};

#[derive(Default)]
pub struct ScoringPage {
    theme: ThemeCache<PageTheme>,
}

struct Tolerances;

impl View for Tolerances {
    type Props<'a> = ScoringThresholds;
    type Theme = PageTheme;
    type Emission = ScoringThresholds;

    fn view(t: &ScoringThresholds, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<Self::Emission>) {
        ui.label(theme.note("Applies to new sessions."));
        let edit = kit::edited(t, |t| {
            grid("scoring", theme).show(ui, |ui| {
                ui.label(RichText::new("Good timing within").color(theme.ok));
                ui.add(slider_field(&mut t.good_ms, 2.0..=40.0).suffix("ms"));
                ui.end_row();
                ui.label(RichText::new("OK timing within").color(theme.warning));
                ui.add(slider_field(&mut t.ok_ms, t.good_ms..=80.0).suffix("ms"));
                ui.end_row();
                ui.label("Matching window");
                ui.add(slider_field(&mut t.window_ms, 40.0..=300.0).suffix("ms"))
                    .on_hover_text("Notes further than this from any target count as extra notes");
                ui.end_row();
                ui.label(RichText::new("Good length within").color(theme.ok));
                let mut g = t.length_good * 100.0;
                if ui
                    .add(slider_field(&mut g, 5.0..=80.0).suffix("%"))
                    .changed()
                {
                    t.length_good = g / 100.0;
                }
                ui.end_row();
                ui.label(RichText::new("OK length within").color(theme.warning));
                let mut o = t.length_ok * 100.0;
                if ui
                    .add(slider_field(&mut o, g..=150.0).suffix("%"))
                    .changed()
                {
                    t.length_ok = o / 100.0;
                }
                ui.end_row();
            });
        });
        ui.label(
            theme.note("Whether note length is scored is part of each exercise's groove settings."),
        );
        if let Some(t) = edit {
            out.send(t);
        }
    }
}

impl PreferencePage for ScoringPage {
    fn id(&self) -> &'static str {
        SCORING
    }
    fn title(&self) -> &'static str {
        "Scoring"
    }
    fn icon(&self) -> &'static str {
        icons::TARGET
    }

    fn ui(&mut self, ui: &mut Ui, ctx: &mut PrefsCtx) {
        let theme = self.theme.get();
        let t: ScoringThresholds = ctx.settings.get(SCORING);
        for t in kit::show::<Tolerances>(ui, "scoring", &t, &theme) {
            ctx.settings.set(SCORING, &t);
        }
    }
}
