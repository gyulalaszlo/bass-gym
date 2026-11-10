//! The tap-tempo button: tap at least 3 times in tempo and it emits the tempo.
//!
//! A tier-2 [`Component`]: the taps are its `State`, a click sends a private
//! [`Msg::Tap`], and [`update`](TapTempo::update) works out the tempo.

use egui::Ui;

use crate::icons;
use crate::kit::{Component, Emit};
use crate::theme::Tokens;
use crate::widgets::button::{self, button};

/// A pause longer than this starts a new series of taps, in seconds.
const RESET_AFTER: f64 = 2.0;
/// Taps remembered: the tempo is their average.
const MAX_TAPS: usize = 8;
const MIN_TAPS: usize = 3;

pub struct TapTempo;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// Times of the recent taps, in seconds of `egui` input time.
    taps: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Props {
    /// Height of the button.
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub button: button::Theme,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            button: button::Theme::normal(t),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Msg {
    /// The button was clicked at this time.
    Tap(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Emission {
    /// A new tempo, in BPM: rounded and within 20 to 400.
    Tempo(f32),
}

impl Component for TapTempo {
    type State = State;
    type Props<'a> = Props;
    type Theme = Theme;
    type Msg = Msg;
    type Emission = Emission;

    fn view(_state: &State, props: &Props, theme: &Theme, ui: &mut Ui, msg: &mut Emit<Msg>) {
        let r = ui
            .add(
                button(format!("{} TAP", icons::HAND_TAP))
                    .theme(&theme.button)
                    .min_size([88.0, props.height]),
            )
            .on_hover_text("Tap at least 3 times in tempo");
        if r.clicked() {
            msg.send(Msg::Tap(ui.input(|i| i.time)));
        }
    }

    fn update(state: &mut State, _props: &Props, msg: Msg, out: &mut Emit<Emission>) {
        let Msg::Tap(now) = msg;
        let taps = &mut state.taps;
        if taps.last().is_some_and(|&t| now - t > RESET_AFTER) {
            taps.clear();
        }
        taps.push(now);
        if taps.len() > MAX_TAPS {
            taps.remove(0);
        }
        if taps.len() >= MIN_TAPS {
            let span = taps[taps.len() - 1] - taps[0];
            let bpm = 60.0 * (taps.len() - 1) as f32 / span as f32;
            out.send(Emission::Tempo(bpm.round().clamp(20.0, 400.0)));
        }
    }
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self::new(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::{Rig, mount};
    use crate::theme::Tokens;

    const PROPS: Props = Props { height: 36.0 };

    /// Taps at `times` and returns the emissions.
    fn tap(state: &mut State, times: &[f64]) -> Vec<Emission> {
        let mut out = Emit::new();
        for &t in times {
            TapTempo::update(state, &PROPS, Msg::Tap(t), &mut out);
        }
        out.into_vec()
    }

    #[test]
    fn needs_three_taps() {
        let mut s = State::default();
        assert!(tap(&mut s, &[0.0, 0.5]).is_empty());
        assert_eq!(tap(&mut s, &[1.0]), vec![Emission::Tempo(120.0)]);
    }

    #[test]
    fn a_long_pause_starts_over() {
        let mut s = State::default();
        tap(&mut s, &[0.0, 0.5, 1.0]);
        assert!(tap(&mut s, &[5.0, 5.6]).is_empty());
        assert_eq!(tap(&mut s, &[6.2]), vec![Emission::Tempo(100.0)]);
    }

    #[test]
    fn averages_the_last_eight_taps_and_clamps() {
        let mut s = State::default();
        let times: Vec<f64> = (0..12).map(|i| i as f64 * 0.1).collect();
        let out = tap(&mut s, &times);
        assert_eq!(out.last(), Some(&Emission::Tempo(400.0)));
        assert_eq!(s.taps.len(), MAX_TAPS);
    }

    #[test]
    fn clicking_the_button_taps() {
        let theme = Theme::new(&Tokens::default());
        let mut state = State::default();
        let mut emitted = Vec::new();
        let mut ui_fn = |ui: &mut Ui| {
            emitted.extend(mount::<TapTempo>(ui, "tap", &mut state, &PROPS, &theme));
        };
        let mut rig = Rig::new();
        let at = egui::pos2(20.0, 20.0);
        rig.frame(Vec::new(), &mut ui_fn);
        for _ in 0..3 {
            rig.click(at, &mut ui_fn);
        }
        drop(ui_fn);
        assert_eq!(state.taps.len(), 3);
        assert_eq!(emitted.len(), 1);
    }
}
