//! The settings: the Scoring section, then the groove panel. The Progression and Piano
//! sections are views too, shown in the toolbar's menus (`view/toolbar.rs`).

pub mod piano;
pub mod progression;
pub mod scoring;

use bg_ui::egui::Ui;
use bg_ui::kit::{self, Emit};
use bg_ui::theme::section;
use bg_ui::widgets::session_panel::{self, SessionPanel};

use crate::basic_chords::msg::Msg;
use crate::basic_chords::props;
use crate::basic_chords::theme::Theme;

pub fn config(p: &props::Config, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    let d = p.data;
    ui.add_enabled_ui(p.editable, |ui| {
        ui.set_max_width(theme.metrics.column_width);
        section(ui, "Scoring", |ui| {
            let props = scoring::Props {
                scoring: &d.scoring,
                checker: &d.checker,
            };
            let msgs = kit::show::<scoring::Scoring>(ui, "scoring", &props, &theme.form);
            out.extend_map(msgs, Msg::Scoring);
        });
    });
    ui.add_space(theme.metrics.columns_gap);
    let props = session_panel::Props {
        cfg: &d.session,
        enabled: p.editable,
    };
    let edits = kit::show::<SessionPanel>(ui, "session", &props, &theme.session);
    out.extend_map(edits, |cfg| Msg::Session(Box::new(cfg)));
}
