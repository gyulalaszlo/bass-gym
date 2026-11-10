//! The settings panels: Progression, Piano and Scoring side by side (stacked when
//! narrow), then the groove panel. Each section is a view over its slice of the data.

pub mod piano;
pub mod progression;
pub mod scoring;

use bg_ui::egui::Ui;
use bg_ui::kit::{self, Emit};
use bg_ui::theme::{responsive_columns, section};
use bg_ui::widgets::session_panel::session_panel;

use crate::basic_chords::msg::Msg;
use crate::basic_chords::props;
use crate::basic_chords::theme::Theme;

pub fn config(p: &props::Config, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    let d = p.data;
    let form = &theme.form;
    ui.add_enabled_ui(p.editable, |ui| {
        responsive_columns(ui, theme.metrics.column_width, 3, |ui, i| match i {
            0 => section(ui, "Progression", |ui| {
                let props = progression::Props {
                    progression: &d.progression,
                    chord_length: d.chord_length,
                    seed: d.seed,
                    new_seed_each_start: d.new_seed_each_start,
                    key: &d.session.key,
                    run_seed: p.run_seed,
                };
                let msgs = kit::show::<progression::Progression>(ui, "progression", &props, form);
                out.extend_map(msgs, Msg::Progression);
            }),
            1 => section(ui, "Piano", |ui| {
                let props = piano::Props {
                    comp: &d.comp,
                    jazziness: d.jazziness,
                    width: d.width,
                    center_octave: d.center_octave,
                    humanize: d.humanize,
                    piano_volume: d.piano_volume,
                    release: d.release,
                    keyboard: &d.keyboard,
                };
                let msgs = kit::show::<piano::Piano>(ui, "piano", &props, form);
                out.extend_map(msgs, Msg::Piano);
            }),
            _ => section(ui, "Scoring", |ui| {
                let props = scoring::Props {
                    scoring: &d.scoring,
                    checker: &d.checker,
                };
                let msgs = kit::show::<scoring::Scoring>(ui, "scoring", &props, form);
                out.extend_map(msgs, Msg::Scoring);
            }),
        });
    });
    ui.add_space(theme.metrics.columns_gap);
    // `session_panel` edits in place (not migrated yet): edit a copy, send it if changed.
    let session = kit::edited(&d.session, |cfg| {
        session_panel(ui, cfg, p.editable);
    });
    if let Some(cfg) = session {
        out.send(Msg::Session(Box::new(cfg)));
    }
}
