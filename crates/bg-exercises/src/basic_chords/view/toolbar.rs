//! The toolbar: start / stop, results, the chord now and next, and the session
//! settings.

use bg_ui::kit::{self, Component, Emit};
use bg_ui::views::tap_tempo::TapTempo;
use bg_ui::widgets::session_panel::session_toolbar;
use bg_ui::widgets::toolbar::{Item, Toolbar};

use super::status::StatusLine;
use crate::basic_chords::msg::Msg;
use crate::basic_chords::props;
use crate::basic_chords::theme::Theme;

pub fn toolbar(p: &props::Toolbar, theme: &Theme, tb: &mut Toolbar, out: &mut Emit<Msg>) {
    if tb.at(Item::Transport, |tb| tb.transport(p.running)) {
        out.send(Msg::Transport);
    }
    if tb.at(Item::Results, |tb| {
        tb.results_button(p.has_results && !p.running)
    }) {
        out.send(Msg::OpenResults);
    }
    tb.at(Item::Status, |tb| {
        if let Some(status) = &p.status {
            kit::show::<StatusLine>(tb.ui(), "status", status, &theme.status);
        }
    });
    let edited = tb.group(!p.running, |tb| {
        // Tap tempo, as tall as the toolbar's buttons. Its messages go through the
        // exercise, which keeps its state.
        let tap = theme.tap.with_height(tb.theme().button_height);
        let mut taps = Emit::new();
        tb.at(Item::Tap, |tb| {
            TapTempo::view(p.tap, &(), &tap, tb.ui(), &mut taps)
        });
        out.extend_map(taps, Msg::Tap);
        session_toolbar(tb, &theme.session, p.session)
    });
    if let Some(cfg) = edited {
        out.send(Msg::Session(Box::new(cfg)));
    }
}
