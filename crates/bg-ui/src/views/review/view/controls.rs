//! Play / stop, the replay options, the playback position and the selected bars.

use egui::{Align, Layout, RichText, Ui};

use super::Props;
use crate::icons;
use crate::kit::{Emit, field};
use crate::views::review::theme::Theme;
use crate::views::review::{Msg, position_at};
use crate::widgets::button::button;
use crate::widgets::controls::{checkbox, number_field};

pub fn controls(p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    let r = p.review;
    let m = &theme.metrics;
    ui.horizontal(|ui| {
        let (icon, label) = if r.is_playing() {
            (icons::STOP, "Stop")
        } else {
            (icons::PLAY, "Play")
        };
        if ui
            .add_enabled(
                r.can_play(),
                button(format!("{icon} {label}")).min_size(m.play_button),
            )
            .on_hover_text("Plays the selected bars (Space). Click the score to play from there.")
            .clicked()
        {
            out.send(Msg::TogglePlay);
        }
        field(ui, &r.click, out, Msg::Click, |ui, v| {
            checkbox(ui, v, "Click")
        });
        field(ui, &r.chords, out, Msg::Chords, |ui, v| {
            checkbox(ui, v, "Chords")
        });
        ui.add_space(m.group_gap);
        let at = r
            .playhead()
            .or(r.selected_range().map(|range| range.0))
            .and_then(|t| position_at(&r.score, t));
        if let Some(at) = at {
            for (value, unit) in [
                (at.bar, "BAR"),
                (at.beat as i64, "BEAT"),
                (at.sub as i64, "1/32"),
            ] {
                ui.label(
                    RichText::new(value.to_string())
                        .font(theme.position_font.clone())
                        .color(theme.position),
                );
                ui.label(
                    RichText::new(unit)
                        .font(theme.unit_font.clone())
                        .color(theme.unit),
                );
            }
        }
        ui.add_space(m.group_gap);
        field(ui, &r.log, out, Msg::Log, |ui, v| {
            checkbox(ui, v, "Log scale")
        })
        .on_hover_text("Shows levels in dB, so quiet parts are easier to read");
        let (first, last) = r.bar_range();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            field(ui, &r.to_bar, out, Msg::ToBar, |ui, v| {
                ui.add(
                    number_field(v)
                        .range(first..=last)
                        .width(m.bar_field_width)
                        .id_salt("to_bar"),
                )
            });
            ui.label("to");
            field(ui, &r.from_bar, out, Msg::FromBar, |ui, v| {
                ui.add(
                    number_field(v)
                        .range(first..=last)
                        .width(m.bar_field_width)
                        .id_salt("from_bar"),
                )
            });
            ui.label("Selected bars");
        });
    });
}
