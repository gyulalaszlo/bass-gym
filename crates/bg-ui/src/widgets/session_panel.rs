//! The shared session controls of every timing exercise: the toolbar part (tempo, time
//! signature, practice options and click menus) and the groove panel.

use bg_core::SessionConfig;
use bg_core::groove::builtin_grooves;
use bg_core::session::{ClickDensity, Span, StopCondition, Trigger};
use egui::{RichText, Slider, Ui};

use crate::widgets::button::selectable;
use crate::icons;
use crate::theme::{self, mono, section};
use crate::widgets::controls::{checkbox, dropdown, dropdown_value, form_row, number_field};
use crate::widgets::groove_editor::groove_editor;
use crate::widgets::toolbar::{Gap, Toolbar};

/// Adds the session settings to the toolbar: tap, BPM, time signature and the practice
/// options and click menus, with their gaps. Returns true if anything changed.
pub fn session_toolbar(tb: &mut Toolbar, cfg: &mut SessionConfig) -> bool {
    let before = cfg.clone();
    tb.tap(&mut cfg.tempo);
    tb.gap(Gap::Medium);
    tb.bpm(&mut cfg.tempo);
    tb.gap(Gap::Small);
    tb.time_sig(&mut cfg.time_sig);
    tb.gap(Gap::Large);
    let practice_on = cfg.ramp.enabled || cfg.gap.enabled || cfg.auto_stop.enabled || cfg.auto_stop.on_silence;
    tb.menu("practice_menu", icons::SLIDERS_HORIZONTAL, "Practice options", practice_on, 400.0, |ui| practice_options(ui, cfg));
    tb.gap(Gap::Small);
    tb.menu("click_menu", icons::METRONOME, "Click", cfg.click.enabled || cfg.groove_click.enabled, 360.0, |ui| click_options(ui, cfg));
    *cfg != before
}

/// The groove panel (picker and step editor). Returns true if anything changed.
pub fn session_panel(ui: &mut Ui, cfg: &mut SessionConfig, enabled: bool) -> bool {
    let before = cfg.clone();
    ui.add_enabled_ui(enabled, |ui| section(ui, "Groove", |ui| groove(ui, cfg)));
    *cfg != before
}

fn click_options(ui: &mut Ui, cfg: &mut SessionConfig) {
    form_row(ui, icons::TIMER, "Count in", |ui| {
        let unit = if cfg.count_in_bars == 1 { "bar" } else { "bars" };
        ui.add(number_field(&mut cfg.count_in_bars).range(0..=4).suffix(unit));
    });
    form_row(ui, icons::METRONOME, "Click every", |ui| {
        dropdown_value(ui, "click_density", &mut cfg.click.density, ClickDensity::ALL, |d| d.label().to_owned());
    });
    form_row(ui, icons::SPEAKER_HIGH, "Click volume", |ui| {
        ui.add(Slider::new(&mut cfg.click.volume, 0.0..=1.0).show_value(false));
    });
    ui.add_space(8.0);
    form_row(ui, icons::METRONOME, "Click", |ui| checkbox(ui, &mut cfg.click.enabled, ""));
    ui.add_enabled_ui(cfg.click.enabled, |ui| {
        form_row(ui, icons::ARROW_FAT_LINE_DOWN, "Accent the downbeat", |ui| checkbox(ui, &mut cfg.click.accent_downbeat, ""));
    });
    form_row(ui, icons::MUSIC_NOTES, "Groove click", |ui| checkbox(ui, &mut cfg.groove_click.enabled, ""));
    ui.add_enabled_ui(cfg.groove_click.enabled, |ui| {
        form_row(ui, "", "Pitch", |ui| {
            let name = |p: &u8| format!("{}{}", bg_core::music::note_name(*p as i32), bg_core::music::note_octave(*p as i32));
            dropdown_value(ui, "groove_click_pitch", &mut cfg.groove_click.pitch, 23..=67u8, name);
        });
        form_row(ui, "", "Volume", |ui| {
            ui.add(Slider::new(&mut cfg.groove_click.volume, 0.0..=1.0).show_value(false));
        });
    });
}

/// An option heading: checkbox, icon, underlined title and a short description.
fn option_heading(ui: &mut Ui, enabled: &mut bool, icon: &str, title: &str, desc: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        checkbox(ui, enabled, "");
        ui.label(RichText::new(icon).color(theme::dim()));
        let r = ui.label(RichText::new(format!("{title}:")).color(theme::text()));
        ui.painter().hline(r.rect.x_range(), r.rect.bottom(), egui::Stroke::new(1.0, theme::text()));
        ui.label(RichText::new(desc).color(theme::text()));
    });
}

/// Indented sentence lines under an option heading.
fn option_body(ui: &mut Ui, enabled: bool, add: impl FnOnce(&mut Ui)) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.indent("body", |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            add(ui);
        });
    });
}

fn practice_options(ui: &mut Ui, cfg: &mut SessionConfig) {
    let r = &mut cfg.ramp;
    option_heading(ui, &mut r.enabled, icons::TRAY_ARROW_UP, "Tempo ramp", "change tempo");
    option_body(ui, r.enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("to");
            ui.add(number_field(&mut r.target).range(20.0..=400.0).step(5.0).decimals(0).suffix("bpm"));
            ui.label("by");
            ui.add(number_field(&mut r.step).range(0.5..=50.0).step(1.0).decimals(1).suffix("bpm"));
        });
        ui.horizontal(|ui| {
            ui.label("every step");
            trigger_editor(ui, "ramp_trigger", &mut r.every);
        });
    });
    ui.add_space(10.0);

    let g = &mut cfg.gap;
    option_heading(ui, &mut g.enabled, icons::SPEAKER_SLASH, "Click gaps", "silence the click periodically");
    option_body(ui, g.enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("play");
            span_editor(ui, "gap_play", &mut g.play);
        });
        ui.horizontal(|ui| {
            ui.label("mute");
            span_editor(ui, "gap_mute", &mut g.mute);
        });
    });
    ui.add_space(10.0);

    let a = &mut cfg.auto_stop;
    option_heading(ui, &mut a.enabled, icons::STOP_CIRCLE, "Auto stop", "end the session");
    option_body(ui, a.enabled, |ui| {
        let is_ramp = matches!(a.when, StopCondition::RampDone { .. });
        ui.horizontal(|ui| {
            if ui.radio(!is_ramp, "").clicked() && is_ramp {
                a.when = StopCondition::Trigger(Trigger::After(Span::Bars(64)));
            }
            trigger_label_editor(ui, a);
        });
        ui.horizontal(|ui| {
            if ui.radio(is_ramp, "").clicked() && !is_ramp {
                a.when = StopCondition::RampDone { bars: 8 };
            }
            ui.label("when the tempo ramp is done, then");
            if let StopCondition::RampDone { bars } = &mut a.when {
                ui.add(number_field(bars).range(0..=999).suffix("bars"));
            }
        });
    });
    ui.add_space(10.0);

    let mut on = a.on_silence;
    option_heading(ui, &mut on, icons::HAND_PALM, "Stop on silence", "when I stop playing");
    a.on_silence = on;
    option_body(ui, a.on_silence, |ui| {
        ui.horizontal(|ui| {
            ui.label("for");
            let unit = if a.silence_bars == 1 { "bar" } else { "bars" };
            ui.add(number_field(&mut a.silence_bars).range(1..=16).suffix(unit));
        });
    });
}

fn trigger_label_editor(ui: &mut Ui, a: &mut bg_core::session::AutoStop) {
    match &mut a.when {
        StopCondition::Trigger(t) => trigger_editor(ui, "stop_trigger", t),
        StopCondition::RampDone { .. } => {
            ui.label(RichText::new("after a time or on success").color(theme::dim()));
        }
    }
}

fn span_editor(ui: &mut Ui, id: &str, span: &mut Span) {
    match span {
        Span::Bars(b) => ui.add(number_field(b).range(1..=999)),
        Span::Seconds(s) => ui.add(number_field(s).range(1.0..=3600.0).step(5.0).decimals(0)),
    };
    let is_bars = matches!(span, Span::Bars(_));
    let mut unit = is_bars;
    dropdown_value(ui, id, &mut unit, [true, false], |b| if *b { "bars".into() } else { "sec".into() });
    if unit != is_bars {
        *span = if unit { Span::Bars(4) } else { Span::Seconds(30.0) };
    }
}

fn trigger_editor(ui: &mut Ui, id: &str, trigger: &mut Trigger) {
    let is_success = matches!(trigger, Trigger::Success { .. });
    let mut success = is_success;
    dropdown_value(ui, id, &mut success, [false, true], |s| if *s { "on success".into() } else { "after".into() });
    if success != is_success {
        *trigger = if success { Trigger::Success { bars: 4, min_rate: 0.9 } } else { Trigger::After(Span::Bars(32)) };
    }
    match trigger {
        Trigger::After(span) => span_editor(ui, &format!("{id}_span"), span),
        Trigger::Success { bars, min_rate } => {
            let mut pct = *min_rate * 100.0;
            ui.add(number_field(&mut pct).range(10.0..=100.0).step(5.0).decimals(0).suffix("% hits"));
            *min_rate = pct / 100.0;
            ui.label("over");
            ui.add(number_field(bars).range(1..=64).suffix("bars"));
        }
    }
}

fn groove(ui: &mut Ui, cfg: &mut SessionConfig) {
    ui.horizontal_wrapped(|ui| {
        dropdown(ui, "groove_pick", RichText::new(&cfg.groove.name), |ui| {
            for g in builtin_grooves(cfg.time_sig) {
                let selected = g.name == cfg.groove.name;
                if ui.add(selectable(selected, &g.name)).clicked() {
                    cfg.groove = g;
                }
            }
        });
        let notes = cfg.groove.notes.len();
        ui.label(RichText::new(format!("{notes} notes / {} bar{}", cfg.groove.bars, if cfg.groove.bars > 1 { "s" } else { "" })).font(mono(13.0)).color(theme::dim()));
    });
    groove_editor(ui, &mut cfg.groove, cfg.time_sig);
}
