//! The toolbar: start / stop, results, the chord now and next, and the session
//! settings.

use bg_ui::kit::{self, Emit};
use bg_ui::widgets::session_panel::session_toolbar;
use bg_ui::widgets::toolbar::{Gap, Toolbar};

use super::status::StatusLine;
use crate::basic_chords::msg::Msg;
use crate::basic_chords::props;
use crate::basic_chords::theme::Theme;

pub fn toolbar(p: &props::Toolbar, theme: &Theme, tb: &mut Toolbar, out: &mut Emit<Msg>) {
    if tb.transport(p.running) {
        out.send(Msg::Transport);
    }
    tb.gap(Gap::Small);
    if tb.results_button(p.has_results && !p.running) {
        out.send(Msg::OpenResults);
    }
    // Current / next chord in the space between START and the settings.
    tb.gap_with(Gap::Large, |ui| {
        if let Some(status) = &p.status {
            kit::show::<StatusLine>(ui, "status", status, &theme.status);
        }
    });
    // `session_toolbar` edits in place (not migrated yet): edit a copy, send it if changed.
    let mut cfg = p.session.clone();
    if tb.group(!p.running, |tb| session_toolbar(tb, &mut cfg)) {
        out.send(Msg::Session(Box::new(cfg)));
    }
}
