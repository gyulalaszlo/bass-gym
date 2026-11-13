//! The toolbar: start / stop, results, the live status and the session settings.

use bg_ui::kit::{self, Component, Emit};
use bg_ui::views::tap_tempo::TapTempo;
use bg_ui::widgets::session_panel::session_toolbar;
use bg_ui::widgets::toolbar::{Item, Toolbar};

use super::status::StatusLine;
use crate::basic_scales::menu::{self, ScaleMenu};
use crate::basic_scales::{CustomScale, ScaleSettings};
use crate::basic_timing::msg::Msg;
use crate::basic_timing::props;
use crate::basic_timing::theme::Theme;
use bg_core::SessionConfig;

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
        kit::show::<StatusLine>(tb.ui(), "status", &p.status, &theme.status)
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
        let edited = session_toolbar(tb, &theme.session, p.session);
        if let Some((s, library)) = &p.scales {
            scale_menu(s, library, p.session, theme, tb, out);
        }
        edited
    });
    if let Some(cfg) = edited {
        out.send(Msg::Session(Box::new(cfg)));
    }
}

/// Width of the Scale menu, and its label's (fixed, so later items don't move).
const MENU_WIDTH: f32 = 380.0;
const LABEL_WIDTH: f32 = 170.0;

/// Basic scales' Scale menu, next to the key.
fn scale_menu(
    s: &ScaleSettings,
    library: &[CustomScale],
    session: &SessionConfig,
    theme: &Theme,
    tb: &mut Toolbar,
    out: &mut Emit<Msg>,
) {
    let label = (s.name(&session.key), tb.theme().summary_font.clone());
    let edits = tb.at(Item::Progression, |tb| {
        tb.menu_labelled(
            "bs_scale",
            label,
            "Scale",
            false,
            Some(LABEL_WIDTH),
            MENU_WIDTH,
            |ui| {
                let props = menu::Props {
                    settings: s,
                    key: &session.key,
                    library,
                };
                kit::show::<ScaleMenu>(ui, "scale", &props, &theme.form)
            },
        )
    });
    for s in edits.into_iter().flatten() {
        out.send(Msg::Scales(Box::new(s)));
    }
}
