//! The shared "tempo + groove" configuration panel used by every timing exercise.

use bg_core::groove::builtin_grooves;
use bg_core::session::{ClickDensity, Span, StopCondition, Trigger};
use bg_core::SessionConfig;
use egui::{DragValue, RichText};

use crate::icons;
use crate::theme::{self, mono, responsive_columns, section};
use crate::widgets::groove_editor::groove_editor;

/// Edits a [`SessionConfig`]. Returns true if anything changed.
pub fn session_panel(ui: &mut egui::Ui, cfg: &mut SessionConfig, enabled: bool) -> bool {
    let before = cfg.clone();
    ui.add_enabled_ui(enabled, |ui| {
        responsive_columns(ui, 360.0, 2, |ui, i| match i {
            0 => section(ui, "Tempo", |ui| tempo(ui, cfg)),
            _ => section(ui, "Click", |ui| click(ui, cfg)),
        });
        section(ui, "Groove", |ui| groove(ui, cfg));
        egui::CollapsingHeader::new(RichText::new(format!("{}  PRACTICE OPTIONS", icons::SLIDERS_HORIZONTAL)).color(theme::text()))
            .id_salt("practice_opts")
            .show(ui, |ui| {
                responsive_columns(ui, 320.0, 3, |ui, i| match i {
                    0 => section(ui, "Tempo ramp", |ui| ramp(ui, cfg)),
                    1 => section(ui, "Click gaps", |ui| gaps(ui, cfg)),
                    _ => section(ui, "Auto stop", |ui| auto_stop(ui, cfg)),
                });
            });
    });
    *cfg != before
}

fn tempo(ui: &mut egui::Ui, cfg: &mut SessionConfig) {
    ui.horizontal_wrapped(|ui| {
        if ui.button(icons::MINUS).clicked() {
            cfg.tempo = (cfg.tempo - 1.0).max(20.0);
        }
        ui.add(DragValue::new(&mut cfg.tempo).range(20.0..=400.0).speed(0.5).fixed_decimals(0).suffix(" BPM"));
        if ui.button(icons::PLUS).clicked() {
            cfg.tempo = (cfg.tempo + 1.0).min(400.0);
        }
        if ui.button(format!("{} Tap", icons::HAND_TAP)).on_hover_text("Tap at least 3 times in tempo").clicked()
            && let Some(bpm) = tap(ui) {
                cfg.tempo = bpm.round().clamp(20.0, 400.0);
            }
    });
    ui.horizontal_wrapped(|ui| {
        theme::caption(ui, "Time");
        ui.add(DragValue::new(&mut cfg.time_sig.num).range(1..=15));
        ui.label("/");
        egui::ComboBox::from_id_salt("sig_den").selected_text(cfg.time_sig.den.to_string()).width(44.0).show_ui(ui, |ui| {
            for d in [2, 4, 8, 16] {
                ui.selectable_value(&mut cfg.time_sig.den, d, d.to_string());
            }
        });
        theme::caption(ui, "Count-in");
        ui.add(DragValue::new(&mut cfg.count_in_bars).range(0..=4).suffix(" bar"));
    });
}

fn tap(ui: &egui::Ui) -> Option<f32> {
    let id = egui::Id::new("tap_tempo");
    let now = ui.input(|i| i.time);
    let mut taps: Vec<f64> = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    if taps.last().is_some_and(|&t| now - t > 2.0) {
        taps.clear();
    }
    taps.push(now);
    if taps.len() > 8 {
        taps.remove(0);
    }
    ui.data_mut(|d| d.insert_temp(id, taps.clone()));
    (taps.len() >= 3).then(|| 60.0 * (taps.len() - 1) as f32 / (taps[taps.len() - 1] - taps[0]) as f32)
}

fn click(ui: &mut egui::Ui, cfg: &mut SessionConfig) {
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut cfg.click.enabled, format!("{} Click", icons::METRONOME));
        egui::ComboBox::from_id_salt("click_density").selected_text(cfg.click.density.label()).width(80.0).show_ui(ui, |ui| {
            for d in ClickDensity::ALL {
                ui.selectable_value(&mut cfg.click.density, d, d.label());
            }
        });
        ui.checkbox(&mut cfg.click.accent_downbeat, "Accent 1");
        ui.add(egui::Slider::new(&mut cfg.click.volume, 0.0..=1.0).show_value(false)).on_hover_text("Click volume");
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut cfg.groove_click.enabled, format!("{} Groove click", icons::MUSIC_NOTES));
        ui.add_enabled_ui(cfg.groove_click.enabled, |ui| {
            let mut pitch = cfg.groove_click.pitch as i32;
            ui.add(
                DragValue::new(&mut pitch)
                    .range(23..=67)
                    .custom_formatter(|v, _| format!("{}{}", bg_core::music::note_name(v as i32), bg_core::music::note_octave(v as i32))),
            )
            .on_hover_text("Groove click pitch");
            cfg.groove_click.pitch = pitch as u8;
            ui.add(egui::Slider::new(&mut cfg.groove_click.volume, 0.0..=1.0).show_value(false)).on_hover_text("Groove click volume");
        });
    });
}

fn groove(ui: &mut egui::Ui, cfg: &mut SessionConfig) {
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("groove_pick").selected_text(RichText::new(&cfg.groove.name).color(theme::cyan())).width(220.0).show_ui(ui, |ui| {
            for g in builtin_grooves(cfg.time_sig) {
                let selected = g.name == cfg.groove.name;
                if ui.selectable_label(selected, &g.name).clicked() {
                    cfg.groove = g;
                }
            }
        });
        let notes = cfg.groove.notes.len();
        ui.label(RichText::new(format!("{notes} notes / {} bar{}", cfg.groove.bars, if cfg.groove.bars > 1 { "s" } else { "" })).font(mono(13.0)).color(theme::dim()));
    });
    groove_editor(ui, &mut cfg.groove, cfg.time_sig);
}

fn span_editor(ui: &mut egui::Ui, id: &str, span: &mut Span) {
    let is_bars = matches!(span, Span::Bars(_));
    egui::ComboBox::from_id_salt(id).selected_text(if is_bars { "bars" } else { "sec" }).width(56.0).show_ui(ui, |ui| {
        if ui.selectable_label(is_bars, "bars").clicked() && !is_bars {
            *span = Span::Bars(4);
        }
        if ui.selectable_label(!is_bars, "sec").clicked() && is_bars {
            *span = Span::Seconds(30.0);
        }
    });
    match span {
        Span::Bars(b) => ui.add(DragValue::new(b).range(1..=999)),
        Span::Seconds(s) => ui.add(DragValue::new(s).range(1.0..=3600.0).speed(1.0)),
    };
}

fn trigger_editor(ui: &mut egui::Ui, id: &str, trigger: &mut Trigger) {
    let is_success = matches!(trigger, Trigger::Success { .. });
    egui::ComboBox::from_id_salt(id).selected_text(if is_success { "on success" } else { "after" }).width(90.0).show_ui(ui, |ui| {
        if ui.selectable_label(!is_success, "after").clicked() && is_success {
            *trigger = Trigger::After(Span::Bars(32));
        }
        if ui.selectable_label(is_success, "on success").clicked() && !is_success {
            *trigger = Trigger::Success { bars: 4, min_rate: 0.9 };
        }
    });
    match trigger {
        Trigger::After(span) => span_editor(ui, &format!("{id}_span"), span),
        Trigger::Success { bars, min_rate } => {
            let mut pct = *min_rate * 100.0;
            ui.add(DragValue::new(&mut pct).range(10.0..=100.0).suffix("% hits"));
            *min_rate = pct / 100.0;
            ui.label("over");
            ui.add(DragValue::new(bars).range(1..=64).suffix(" bars"));
        }
    }
}

fn ramp(ui: &mut egui::Ui, cfg: &mut SessionConfig) {
    let r = &mut cfg.ramp;
    ui.checkbox(&mut r.enabled, "Change tempo");
    ui.add_enabled_ui(r.enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("to");
            ui.add(DragValue::new(&mut r.target).range(20.0..=400.0).suffix(" BPM"));
            ui.label("by");
            ui.add(DragValue::new(&mut r.step).range(0.5..=50.0).speed(0.5).suffix(" BPM"));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("every step");
            trigger_editor(ui, "ramp_trigger", &mut r.every);
        });
    });
}

fn gaps(ui: &mut egui::Ui, cfg: &mut SessionConfig) {
    let g = &mut cfg.gap;
    ui.checkbox(&mut g.enabled, "Silence the click periodically");
    ui.add_enabled_ui(g.enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("play");
            span_editor(ui, "gap_play", &mut g.play);
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("mute");
            span_editor(ui, "gap_mute", &mut g.mute);
        });
    });
}

fn auto_stop(ui: &mut egui::Ui, cfg: &mut SessionConfig) {
    let a = &mut cfg.auto_stop;
    ui.checkbox(&mut a.enabled, "Stop automatically");
    ui.add_enabled_ui(a.enabled, |ui| {
        let is_ramp = matches!(a.when, StopCondition::RampDone { .. });
        ui.horizontal_wrapped(|ui| {
            if ui.radio(!is_ramp, "when").clicked() && is_ramp {
                a.when = StopCondition::Trigger(Trigger::After(Span::Bars(64)));
            }
            if let StopCondition::Trigger(t) = &mut a.when {
                trigger_editor(ui, "stop_trigger", t);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.radio(is_ramp, "tempo ramp done, then").clicked() && !is_ramp {
                a.when = StopCondition::RampDone { bars: 8 };
            }
            if let StopCondition::RampDone { bars } = &mut a.when {
                ui.add(DragValue::new(bars).range(0..=999).suffix(" bars"));
            }
        });
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut a.on_silence, "Stop when I stop playing for");
        let unit = if a.silence_bars == 1 { " bar" } else { " bars" };
        ui.add_enabled(a.on_silence, DragValue::new(&mut a.silence_bars).range(1..=16).suffix(unit));
    });
}
