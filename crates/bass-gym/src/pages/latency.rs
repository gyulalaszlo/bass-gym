//! The Latency & detection page: the latency correction (with a measurement), the note
//! detector and note ends.

use std::sync::{Arc, Mutex};

use bg_audio::{AnalysisConfig, CALIBRATION_CLICKS, Calibration, CalibrationState, InputMeter};
use bg_ui::egui::{self, RichText, Ui};
use bg_ui::kit::{self, Emit, ThemeCache, View};
use bg_ui::theme::section;
use bg_ui::widgets::button::button;
use bg_ui::widgets::controls::{number_field, slider_field};
use bg_ui::{PreferencePage, PrefsCtx, icons};

use super::{ANALYSIS, PageTheme, help};

/// How long "Applied" shows after applying a measurement, in seconds.
const APPLIED_FOR: f64 = 4.0;

#[derive(Default)]
pub struct LatencyPage {
    /// Shared with the calibration session, which writes it.
    calibration: Arc<Mutex<CalibrationState>>,
    /// When a measurement was last applied (for a short confirmation).
    applied_at: Option<f64>,
    theme: ThemeCache<PageTheme>,
}

/// Half the range of `v`.
fn spread(v: &[f32]) -> f32 {
    let lo = v.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if v.is_empty() { 0.0 } else { (hi - lo) / 2.0 }
}

/// The Latency correction section.
struct Correction;

struct CorrectionProps<'a> {
    cfg: &'a AnalysisConfig,
    /// A snapshot of the calibration.
    cal: &'a CalibrationState,
    /// A session (an exercise's) is running, so measuring can't start.
    session_running: bool,
    /// "Applied" is showing.
    applied: bool,
}

enum CorrectionEdit {
    Config(AnalysisConfig),
    Measure,
    Cancel,
    /// Use the measured correction, in ms.
    Apply(f32),
    Discard,
}

impl View for Correction {
    type Props<'a> = CorrectionProps<'a>;
    type Theme = PageTheme;
    type Emission = CorrectionEdit;

    fn view(p: &CorrectionProps, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<CorrectionEdit>) {
        let cal = p.cal;
        section(ui, "Latency correction", |ui| {
            help(
                ui,
                theme,
                "The audio driver reports how long sound takes to leave and to come back into the app, and \
                 bass-gym already allows for that. This correction is whatever the driver gets wrong: \
                 positive if the round trip is longer than reported, negative if the driver over-reports \
                 (common with small buffers). Measure it once per audio setup.",
            );
            ui.add_space(theme.gap);
            ui.horizontal(|ui| {
                ui.label("Correction");
                let latency = |ms| {
                    CorrectionEdit::Config(AnalysisConfig {
                        latency_ms: ms,
                        ..p.cfg.clone()
                    })
                };
                kit::field(ui, &p.cfg.latency_ms, out, latency, |ui, v| {
                    ui.add(
                        number_field(v)
                            .range(-200.0..=500.0)
                            .step(0.1)
                            .decimals(1)
                            .suffix("ms")
                            .width(64.0),
                    )
                })
                .on_hover_text("↑ / ↓: ±0.1 ms, Shift: ±1 ms");
                ui.add_space(12.0);
                if cal.running {
                    let text = format!(
                        "{} / {} clicks · {} notes heard",
                        cal.clicks_done,
                        CALIBRATION_CLICKS,
                        cal.offsets_ms.len()
                    );
                    ui.add(
                        egui::ProgressBar::new(cal.clicks_done as f32 / CALIBRATION_CLICKS as f32)
                            .desired_width(240.0)
                            .text(text),
                    );
                    if ui.add(button("Cancel")).clicked() {
                        out.send(CorrectionEdit::Cancel);
                    }
                    ui.ctx().request_repaint();
                } else if ui
                    .add_enabled(
                        !p.session_running,
                        button(format!("{} Measure", icons::GAUGE)),
                    )
                    .on_hover_text("Plays 4 lead-in clicks, then 16 clicks to measure against")
                    .clicked()
                {
                    out.send(CorrectionEdit::Measure);
                }
            });

            if !cal.running && cal.finished {
                match cal.result_ms {
                    Some(r) => {
                        let spread = spread(&cal.offsets_ms);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Measured {r:+.1} ms"))
                                    .font(theme.value_font.clone())
                                    .color(theme.ok),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "(±{spread:.1} ms, {} of {CALIBRATION_CLICKS} clicks)",
                                    cal.offsets_ms.len()
                                ))
                                .color(theme.note),
                            );
                            if ui.add(button(format!("{} Apply", icons::CHECK))).clicked() {
                                out.send(CorrectionEdit::Apply((r * 10.0).round() / 10.0));
                            }
                            if ui.add(button("Discard")).clicked() {
                                out.send(CorrectionEdit::Discard);
                            }
                        });
                    }
                    None => {
                        ui.horizontal(|ui| {
                            let text = format!(
                                "{} Only {} of {CALIBRATION_CLICKS} clicks were matched by a note; check the input level and try again.",
                                icons::WARNING,
                                cal.offsets_ms.len()
                            );
                            ui.label(RichText::new(text).color(theme.warning));
                            if ui.add(button(icons::X).small()).on_hover_text("Dismiss").clicked() {
                                out.send(CorrectionEdit::Discard);
                            }
                        });
                    }
                }
            } else if p.applied {
                ui.label(RichText::new(format!("{} Applied", icons::CHECK_CIRCLE)).color(theme.ok));
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
            }

            ui.add_space(theme.gap);
            help(
                ui,
                theme,
                "To measure: connect the output to the input with a cable (loopback), or pluck a muted \
                 string exactly with each click. Then fine-tune by ear while playing: if your notes show \
                 up late, raise the value; if they show up early, lower it.",
            );
        });
    }
}

/// What the detection sections show: the settings and the live input meter.
struct MeterProps<'a> {
    cfg: &'a AnalysisConfig,
    meter: &'a InputMeter,
}

/// The Note detection section: the gate and onset settings, with the input level.
struct Detection;

impl View for Detection {
    type Props<'a> = MeterProps<'a>;
    type Theme = PageTheme;
    type Emission = AnalysisConfig;

    fn view(p: &MeterProps, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<AnalysisConfig>) {
        let meter = p.meter;
        let edit = kit::edited(p.cfg, |cfg| {
            section(ui, "Note detection", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Input level");
                    let level = ((meter.peak_db + 60.0) / 60.0).clamp(0.0, 1.0);
                    ui.add(
                        egui::ProgressBar::new(level)
                            .desired_width(220.0)
                            .text(format!("{:.0} dBFS", meter.peak_db)),
                    );
                    let (txt, c) = if meter.note_active {
                        ("NOTE", theme.ok)
                    } else {
                        ("—", theme.note)
                    };
                    ui.label(
                        RichText::new(txt)
                            .font(theme.indicator_font.clone())
                            .color(c),
                    );
                });
                egui::Grid::new("detector")
                    .num_columns(2)
                    .spacing(theme.tight_grid_spacing)
                    .show(ui, |ui| {
                        ui.label("Noise gate");
                        ui.add(
                            slider_field(&mut cfg.detector.gate_db, -80.0..=-10.0).suffix("dBFS"),
                        );
                        ui.end_row();
                        ui.label("Onset sensitivity");
                        ui.add(
                            slider_field(&mut cfg.detector.onset_rise_db, 2.0..=15.0)
                                .suffix("dB rise"),
                        )
                        .on_hover_text("Lower = more sensitive");
                        ui.end_row();
                        ui.label("Min. gap");
                        ui.add(
                            slider_field(&mut cfg.detector.min_gap_ms, 20.0..=150.0).suffix("ms"),
                        );
                        ui.end_row();
                        ui.label("String noise");
                        ui.add(
                            slider_field(&mut cfg.detector.noise_db, 0.0..=30.0)
                                .step_by(1.0)
                                .suffix("dB below notes"),
                        )
                        .on_hover_text(
                            "Sounds this far below the recent notes are string noise, not notes \
                             (shown on the waveform in the results, never graded). 0 = off",
                        );
                        ui.end_row();
                        ui.label("Tuner gate");
                        ui.add(slider_field(&mut cfg.tuner_gate_db, -90.0..=-20.0).suffix("dBFS"));
                        ui.end_row();
                    });
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
            });
        });
        if let Some(cfg) = edit {
            out.send(cfg);
        }
    }
}

/// The Note end section: when a note counts as ended.
struct NoteEnd;

impl View for NoteEnd {
    type Props<'a> = MeterProps<'a>;
    type Theme = PageTheme;
    type Emission = AnalysisConfig;

    fn view(p: &MeterProps, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<AnalysisConfig>) {
        let meter = p.meter;
        let edit = kit::edited(p.cfg, |cfg| {
            section(ui, "Note end (note-off)", |ui| {
                ui.label(
                    RichText::new(
                        "A note ends when its level drops the threshold below its peak. Soft, gradual muting \
                         needs a smaller drop; hard, fast muting works with a larger one. The offset shifts \
                         every detected end, if lengths are consistently long or short.",
                    )
                    .small()
                    .color(theme.note),
                );
                egui::Grid::new("note_end")
                    .num_columns(2)
                    .spacing(theme.grid_spacing)
                    .show(ui, |ui| {
                        ui.label("Threshold");
                        ui.add(
                            slider_field(&mut cfg.detector.release_db, 3.0..=40.0)
                                .step_by(0.5)
                                .suffix("dB below peak"),
                        );
                        ui.end_row();
                        ui.label("End offset");
                        ui.add(
                            slider_field(&mut cfg.detector.end_offset_ms, -60.0..=60.0)
                                .step_by(1.0)
                                .suffix("ms"),
                        )
                        .on_hover_text("Negative = notes end earlier");
                        ui.end_row();
                        ui.label("Last note");
                        let text = meter
                            .last_note_ms
                            .map_or("--".to_owned(), |ms| format!("{ms:.0}"));
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(text)
                                    .font(theme.value_font.clone())
                                    .color(theme.ok),
                            );
                            ui.label(
                                RichText::new("MS")
                                    .font(theme.unit_font.clone())
                                    .color(theme.unit),
                            );
                            ui.label(
                                RichText::new("(pluck and mute a note to check)")
                                    .small()
                                    .color(theme.note),
                            );
                        });
                        ui.end_row();
                    });
            });
        });
        if let Some(cfg) = edit {
            out.send(cfg);
        }
    }
}

impl LatencyPage {
    fn reset_calibration(&self) {
        if let Ok(mut c) = self.calibration.lock() {
            *c = CalibrationState::default();
        }
    }
}

impl PreferencePage for LatencyPage {
    fn id(&self) -> &'static str {
        ANALYSIS
    }
    fn title(&self) -> &'static str {
        "Latency & detection"
    }
    fn icon(&self) -> &'static str {
        icons::TIMER
    }

    fn ui(&mut self, ui: &mut Ui, ctx: &mut PrefsCtx) {
        let theme = self.theme.get();
        // Sync: the settings, the calibration's progress and the input meter.
        let mut cfg: AnalysisConfig = ctx.settings.get(ANALYSIS);
        cfg.normalize();
        let cal = self
            .calibration
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        let now = ui.input(|i| i.time);
        if self.applied_at.is_some_and(|t| now - t >= APPLIED_FOR) {
            self.applied_at = None;
        }
        let meter = ctx.engine.meter();

        // View.
        let props = CorrectionProps {
            cfg: &cfg,
            cal: &cal,
            session_running: ctx.engine.session_running(),
            applied: self.applied_at.is_some(),
        };
        let correction = kit::show::<Correction>(ui, "correction", &props, &theme);
        ui.add_space(theme.section_gap);
        let props = MeterProps {
            cfg: &cfg,
            meter: &meter,
        };
        let mut configs = kit::show::<Detection>(ui, "detection", &props, &theme);
        ui.add_space(theme.section_gap);
        configs.extend(kit::show::<NoteEnd>(ui, "note_end", &props, &theme));

        // Update and effects.
        // Normalising can change the stored settings; keep that, as an edit would.
        let stored: AnalysisConfig = ctx.settings.get(ANALYSIS);
        let mut changed = configs
            .pop()
            .or_else(|| (cfg != stored).then(|| cfg.clone()));
        for m in correction {
            match m {
                CorrectionEdit::Config(c) => changed = Some(c),
                CorrectionEdit::Measure => {
                    self.applied_at = None;
                    ctx.engine
                        .start_session(Box::new(Calibration::new(self.calibration.clone())));
                }
                CorrectionEdit::Cancel => {
                    ctx.engine.stop_session();
                    self.reset_calibration();
                }
                CorrectionEdit::Apply(ms) => {
                    let mut c = changed.take().unwrap_or_else(|| cfg.clone());
                    c.latency_ms = ms;
                    changed = Some(c);
                    self.applied_at = Some(now);
                    self.reset_calibration();
                }
                CorrectionEdit::Discard => self.reset_calibration(),
            }
        }
        if let Some(c) = changed
            && c != stored
        {
            ctx.settings.set(ANALYSIS, &c);
            ctx.engine.set_analysis(c);
        }
    }
}
