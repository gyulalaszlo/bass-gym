//! The toolbar: start / stop, results, the chord now and next, the session settings,
//! and the progression and piano menus.

use bg_ui::icons;
use bg_ui::kit::{self, Component, Emit};
use bg_ui::views::tap_tempo::TapTempo;
use bg_ui::widgets::session_panel::session_toolbar;
use bg_ui::widgets::toolbar::{Item, Toolbar};

use super::config::{piano, progression};
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
        let edited = session_toolbar(tb, &theme.session, p.session);
        menus(p, theme, tb, out);
        edited
    });
    if let Some(cfg) = edited {
        out.send(Msg::Session(Box::new(cfg)));
    }
}

/// Width of the progression and piano menus.
const MENU_WIDTH: f32 = 380.0;

/// Fixed width of the progression menu's label, so new numerals (e.g. from Jazziness)
/// don't move the piano menu.
const PROGRESSION_LABEL_WIDTH: f32 = 150.0;

/// The progression menu, next to the key, and the piano menu.
fn menus(p: &props::Toolbar, theme: &Theme, tb: &mut Toolbar, out: &mut Emit<Msg>) {
    let d = p.data;
    let label = (
        p.progression_label.clone(),
        // tb.theme().menu_label_font.clone(),
        tb.theme().summary_font.clone(),
    );
    let msgs = tb.at(Item::Progression, |tb| {
        tb.menu_labelled(
            "bc_progression",
            label,
            "Progression",
            false,
            Some(PROGRESSION_LABEL_WIDTH),
            MENU_WIDTH,
            |ui| {
                let props = progression::Props {
                    progression: &d.progression,
                    chord_length: d.chord_length,
                    seed: d.seed,
                    new_seed_each_start: d.new_seed_each_start,
                    key: &d.session.key,
                    run_seed: p.run_seed,
                };
                kit::show::<progression::Progression>(ui, "progression", &props, &theme.form)
            },
        )
    });
    if let Some(msgs) = msgs {
        out.extend_map(msgs, Msg::Progression);
    }
    let msgs = tb.at(Item::Piano, |tb| {
        tb.menu(
            "bc_piano",
            icons::PIANO_KEYS,
            "Piano",
            false,
            MENU_WIDTH,
            |ui| {
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
                kit::show::<piano::Piano>(ui, "piano", &props, &theme.form)
            },
        )
    });
    if let Some(msgs) = msgs {
        out.extend_map(msgs, Msg::Piano);
    }
}
