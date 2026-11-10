//! The live status in the toolbar: count-in, or the bar, its tempo and whether the
//! click is off. A tier-1 view.

use std::convert::Infallible;

use bg_core::BarPlan;
use bg_ui::egui::{Color32, FontId, RichText, Ui};
use bg_ui::icons;
use bg_ui::kit::{Emit, View};
use bg_ui::theme::Tokens;

pub struct StatusLine;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Nothing to say (no audio output).
    Hidden,
    Ready,
    CountIn,
    Bar {
        number: i64,
        bpm: f32,
        muted: bool,
    },
}

impl Status {
    /// The status of a running session at time `now`.
    pub fn at(bars: &[BarPlan], now: f64) -> Self {
        match bars.iter().rev().find(|b| b.start <= now) {
            Some(b) if b.is_count_in() => Status::CountIn,
            Some(b) => Status::Bar {
                number: b.index + 1,
                bpm: b.bpm,
                muted: b.muted,
            },
            None => Status::Ready,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub font: FontId,
    pub ready: Color32,
    pub count_in: Color32,
    pub bar: Color32,
    pub bpm: Color32,
    pub muted: Color32,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            font: t.mono(16.0),
            ready: t.dim(),
            count_in: t.amber(),
            bar: t.text(),
            bpm: t.green(),
            muted: t.amber(),
        }
    }
}

impl View for StatusLine {
    type Props<'a> = Status;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(status: &Status, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        // Labels go straight into the parent's row (the toolbar lays out left to right).
        let text = |t: String, c| RichText::new(t).font(theme.font.clone()).color(c);
        match status {
            Status::Hidden => {}
            Status::Ready => {
                ui.label(text("READY".into(), theme.ready));
            }
            Status::CountIn => {
                ui.label(text("COUNT-IN".into(), theme.count_in));
            }
            Status::Bar { number, bpm, muted } => {
                ui.label(text(format!("BAR {number:>3}"), theme.bar));
                ui.label(text(format!("{bpm:>5.1} BPM"), theme.bpm));
                if *muted {
                    ui.label(text(
                        format!("{} CLICK OFF", icons::SPEAKER_SLASH),
                        theme.muted,
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_core::SessionConfig;

    #[test]
    fn status_follows_the_bars() {
        let bars = crate::timing_session::preview(&SessionConfig::default()).0;
        assert_eq!(Status::at(&bars, bars[0].start - 1.0), Status::Ready);
        let first = &bars[0];
        let expected = if first.is_count_in() {
            Status::CountIn
        } else {
            Status::Bar {
                number: first.index + 1,
                bpm: first.bpm,
                muted: first.muted,
            }
        };
        assert_eq!(Status::at(&bars, first.start), expected);
    }
}
