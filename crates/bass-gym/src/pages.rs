//! Built-in preference pages.

use std::sync::{Arc, Mutex};

use bg_audio::{AnalysisConfig, AudioConfig, CALIBRATION_CLICKS, Calibration, CalibrationState, DeviceLists, LOWPASS_OFF, SoundConfig};
use bg_core::ScoringThresholds;
use bg_sound::SoundRole;
use bg_ui::theme::{PRESETS, Palette, Rgb, ThemePrefs};
use bg_ui::widgets::tuner::TunerPrefs;
use bg_ui::{PreferencePage, PrefsCtx, egui, icons, theme};
use egui::{DragValue, RichText, Slider};

pub const AUDIO: &str = "audio";
pub const SOUNDS: &str = "sounds";
pub const ANALYSIS: &str = "analysis";
pub const SCORING: &str = "scoring";

pub fn builtin() -> Vec<Box<dyn PreferencePage>> {
    vec![Box::new(DisplayPage), Box::new(AudioPage::default()), Box::new(LatencyPage::default()), Box::new(TunerPage), Box::new(ScoringPage)]
}

fn status_row(ui: &mut egui::Ui, label: &str, ok: Option<&String>, err: Option<&String>, detail: String) {
    ui.horizontal(|ui| {
        match (ok, err) {
            (Some(name), _) => {
                ui.label(RichText::new(icons::CHECK_CIRCLE).color(theme::green()));
                ui.label(format!("{label}: {name}"));
                ui.label(RichText::new(detail).small().color(theme::dim()));
            }
            (None, Some(e)) => {
                ui.label(RichText::new(icons::WARNING).color(theme::amber()));
                ui.label(RichText::new(format!("{label}: {e}")).color(theme::amber()));
            }
            (None, None) => {
                ui.label(RichText::new(format!("{label}: not open")).color(theme::dim()));
            }
        }
    });
}

#[derive(Default)]
pub struct AudioPage {
    devices: Option<DeviceLists>,
}

impl PreferencePage for AudioPage {
    fn id(&self) -> &'static str {
        AUDIO
    }
    fn title(&self) -> &'static str {
        "Audio"
    }
    fn icon(&self) -> &'static str {
        icons::SPEAKER_HIGH
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut PrefsCtx) {
        let devices = self.devices.get_or_insert_with(|| ctx.engine.devices()).clone();
        let mut cfg: AudioConfig = ctx.settings.get(AUDIO);
        let mut sounds: SoundConfig = ctx.settings.get(SOUNDS);

        theme::section(ui, "Devices", |ui| {
            egui::Grid::new("audio_devices").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label(format!("{} Bass input", icons::MICROPHONE));
                device_combo(ui, "in_dev", &mut cfg.input_device, &devices.inputs);
                ui.end_row();

                ui.label("Input channel");
                let channels = devices
                    .inputs
                    .iter()
                    .find(|d| Some(&d.id) == cfg.input_device.as_ref())
                    .map(|d| d.channels)
                    .unwrap_or(ctx.engine.status().input_channels.max(1));
                let text = cfg.input_channel.map_or("Mix all".to_owned(), |c| format!("Channel {}", c + 1));
                egui::ComboBox::from_id_salt("in_ch").selected_text(text).show_ui(ui, |ui| {
                    ui.selectable_value(&mut cfg.input_channel, None, "Mix all");
                    for c in 0..channels {
                        ui.selectable_value(&mut cfg.input_channel, Some(c), format!("Channel {}", c + 1));
                    }
                });
                ui.end_row();

                ui.label(format!("{} Output", icons::SPEAKER_HIGH));
                device_combo(ui, "out_dev", &mut cfg.output_device, &devices.outputs);
                ui.end_row();

                ui.label("Sample rate");
                let text = cfg.sample_rate.map_or("Device default".to_owned(), |r| format!("{r} Hz"));
                egui::ComboBox::from_id_salt("rate").selected_text(text).show_ui(ui, |ui| {
                    ui.selectable_value(&mut cfg.sample_rate, None, "Device default");
                    for r in [44_100, 48_000, 88_200, 96_000] {
                        ui.selectable_value(&mut cfg.sample_rate, Some(r), format!("{r} Hz"));
                    }
                });
                ui.end_row();

                ui.label("Buffer size");
                let text = cfg.buffer_frames.map_or("Device default".to_owned(), |f| format!("{f} frames"));
                egui::ComboBox::from_id_salt("buf").selected_text(text).show_ui(ui, |ui| {
                    ui.selectable_value(&mut cfg.buffer_frames, None, "Device default");
                    for f in [64, 128, 256, 512, 1024] {
                        ui.selectable_value(&mut cfg.buffer_frames, Some(f), format!("{f} frames"));
                    }
                });
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button(format!("{} Rescan devices", icons::ARROWS_CLOCKWISE)).clicked() {
                    self.devices = None;
                }
                if ui.button(format!("{} Reopen audio", icons::PLUGS)).clicked() {
                    ctx.engine.rebuild();
                }
            });
            let st = ctx.engine.status().clone();
            status_row(ui, "Output", st.output.as_ref(), st.output_error.as_ref(), format!("{} Hz", st.output_rate));
            status_row(ui, "Input", st.input.as_ref(), st.input_error.as_ref(), format!("{} Hz, {} ch", st.input_rate, st.input_channels));
            if st.shared_device {
                ui.label(RichText::new("Input and output share a device: both use the output's sample rate and buffer size.").small().color(theme::dim()));
            }
            let xruns = ctx.engine.xruns();
            if xruns > 0 {
                ui.label(RichText::new(format!("{} {xruns} buffer under/overruns since the audio was opened — try a larger buffer size.", icons::WARNING)).small().color(theme::amber()));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            }
            if st.input_rate != 0 && st.output_rate != 0 && st.input_rate != st.output_rate {
                ui.label(RichText::new("Input and output run at different sample rates; timing still works, but a shared rate is more stable.").small().color(theme::amber()));
            }
        });

        ui.add_space(6.0);
        theme::section(ui, "Sounds", |ui| {
            egui::Grid::new("sounds").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label(format!("{} Click", icons::METRONOME));
                sound_combo(ui, "snd_click", &mut sounds.click, SoundRole::Click);
                ui.end_row();
                ui.label(format!("{} Groove click", icons::MUSIC_NOTES));
                sound_combo(ui, "snd_groove", &mut sounds.groove, SoundRole::Click);
                ui.end_row();
                ui.label(format!("{} Instrument", icons::PIANO_KEYS));
                sound_combo(ui, "snd_inst", &mut sounds.instrument, SoundRole::Pitched);
                ui.end_row();
                ui.label("Instrument tone");
                lowpass_slider(ui, &mut sounds);
                ui.end_row();
                ui.label("Master volume");
                ui.add(Slider::new(&mut sounds.master, 0.0..=1.0));
                ui.end_row();
                ui.label(format!("{} Monitor input", icons::EAR));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut sounds.monitor, "");
                    ui.add_enabled(sounds.monitor, Slider::new(&mut sounds.monitor_gain, 0.0..=2.0));
                });
                ui.end_row();
            });
        });

        if let Some(dir) = crate::soundfonts::user_dir() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("More instruments: put SoundFont (.sf2) files in {} and restart.", dir.display())).small().color(theme::dim()));
                if ui.small_button(format!("{} Open folder", icons::FOLDER_OPEN)).clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    open_folder(&dir);
                }
            });
        }

        ctx.settings.set(AUDIO, &cfg);
        ctx.settings.set(SOUNDS, &sounds);
        ctx.engine.set_config(cfg);
        ctx.engine.set_sounds(sounds);
    }
}

fn device_combo(ui: &mut egui::Ui, id: &str, value: &mut Option<String>, list: &[bg_audio::DeviceInfo]) {
    let text = match value {
        None => "System default".to_owned(),
        Some(v) => list.iter().find(|d| &d.id == v).map_or_else(|| "(unavailable device)".to_owned(), |d| d.name.clone()),
    };
    egui::ComboBox::from_id_salt(id).selected_text(text).width(320.0).show_ui(ui, |ui| {
        ui.selectable_value(value, None, "System default");
        for d in list {
            ui.selectable_value(value, Some(d.id.clone()), format!("{}  ({} ch, {} Hz)", d.name, d.channels, d.default_rate));
        }
    });
}

/// Low-pass cutoff for the current instrument, on a log scale; the top end is "off".
fn lowpass_slider(ui: &mut egui::Ui, sounds: &mut SoundConfig) {
    let hz = sounds.lowpass_setting(&sounds.instrument);
    let mut pos = (hz.clamp(400.0, LOWPASS_OFF) / 400.0).log2();
    let max = (LOWPASS_OFF / 400.0).log2();
    let fmt = |p: f64, _: std::ops::RangeInclusive<usize>| {
        let hz = 400.0 * 2f64.powf(p);
        if hz >= LOWPASS_OFF as f64 - 1.0 { "Open".to_owned() } else if hz >= 1000.0 { format!("{:.1} kHz", hz / 1000.0) } else { format!("{hz:.0} Hz") }
    };
    let resp = ui
        .add(Slider::new(&mut pos, 0.0..=max).custom_formatter(fmt).custom_parser(|_| None).text("low-pass"))
        .on_hover_text("Darkens the instrument: a lower cutoff removes more of the bright overtones. Remembered per instrument.");
    if resp.changed() {
        sounds.instrument_lowpass.insert(sounds.instrument.clone(), 400.0 * 2f32.powf(pos));
    }
}

fn open_folder(dir: &std::path::Path) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(cmd).arg(dir).spawn() {
        log::warn!("couldn't open {}: {e}", dir.display());
    }
}

fn sound_combo(ui: &mut egui::Ui, id: &str, value: &mut String, role: SoundRole) {
    let text = bg_sound::source(value).map_or(value.clone(), |s| s.name().to_owned());
    egui::ComboBox::from_id_salt(id).selected_text(text).width(200.0).show_ui(ui, |ui| {
        for s in bg_sound::sources_for(role) {
            ui.selectable_value(value, s.id().to_owned(), s.name());
        }
    });
}

#[derive(Default)]
pub struct LatencyPage {
    calibration: Arc<Mutex<CalibrationState>>,
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

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut PrefsCtx) {
        let mut cfg: AnalysisConfig = ctx.settings.get(ANALYSIS);
        let cal = self.calibration.lock().map(|c| c.clone()).unwrap_or_default();

        theme::section(ui, "Latency", |ui| {
            ui.label(
                RichText::new(
                    "The round trip from the click leaving the app to your note coming back in. \
                     Measure it with a cable from the output to the input (loopback), or by plucking \
                     a muted string exactly with each click.",
                )
                .small()
                .color(theme::dim()),
            );
            egui::Grid::new("latency").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Measured");
                ui.add(DragValue::new(&mut cfg.latency_ms).range(0.0..=500.0).speed(0.1).suffix(" ms"));
                ui.end_row();
                ui.label("Trim");
                ui.add(Slider::new(&mut cfg.latency_trim_ms, -50.0..=50.0).suffix(" ms"));
                ui.end_row();
                ui.label("Total");
                ui.label(RichText::new(format!("{:.1} ms", cfg.latency_ms + cfg.latency_trim_ms)).font(theme::mono(16.0)).color(theme::green()));
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if cal.running {
                    ui.add(egui::ProgressBar::new(cal.clicks_done as f32 / CALIBRATION_CLICKS as f32).desired_width(200.0).text(format!("{} / {} clicks · {} notes heard", cal.clicks_done, CALIBRATION_CLICKS, cal.offsets_ms.len())));
                    if ui.button("Cancel").clicked() {
                        ctx.engine.stop_session();
                    }
                    ui.ctx().request_repaint();
                } else if ui.add_enabled(!ctx.engine.session_running(), egui::Button::new(format!("{} Measure", icons::GAUGE))).clicked() {
                    ctx.engine.start_session(Box::new(Calibration::new(self.calibration.clone())));
                }
            });
            if !cal.running {
                match (cal.result_ms, cal.offsets_ms.is_empty()) {
                    (Some(r), _) => {
                        let spread = spread(&cal.offsets_ms);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Result {r:.1} ms (spread ±{spread:.1} ms, {} notes)", cal.offsets_ms.len())).color(theme::green()));
                            if ui.button(format!("{} Use", icons::CHECK)).clicked() {
                                cfg.latency_ms = r.max(0.0);
                                if let Ok(mut c) = self.calibration.lock() {
                                    c.result_ms = None;
                                }
                            }
                        });
                    }
                    (None, false) => {
                        ui.label(RichText::new(format!("Only {} notes were detected; check the input level and try again.", cal.offsets_ms.len())).color(theme::amber()));
                    }
                    _ => {}
                }
            }
        });

        ui.add_space(6.0);
        theme::section(ui, "Note detection", |ui| {
            let meter = ctx.engine.meter();
            ui.horizontal(|ui| {
                ui.label("Input level");
                let level = ((meter.peak_db + 60.0) / 60.0).clamp(0.0, 1.0);
                ui.add(egui::ProgressBar::new(level).desired_width(220.0).text(format!("{:.0} dBFS", meter.peak_db)));
                let (txt, c) = if meter.note_active { ("NOTE", theme::green()) } else { ("—", theme::dim()) };
                ui.label(RichText::new(txt).font(theme::mono(12.0)).color(c));
            });
            egui::Grid::new("detector").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Noise gate");
                ui.add(Slider::new(&mut cfg.detector.gate_db, -80.0..=-10.0).suffix(" dBFS"));
                ui.end_row();
                ui.label("Onset sensitivity");
                ui.add(Slider::new(&mut cfg.detector.onset_rise_db, 2.0..=15.0).suffix(" dB rise")).on_hover_text("Lower = more sensitive");
                ui.end_row();
                ui.label("Min. gap");
                ui.add(Slider::new(&mut cfg.detector.min_gap_ms, 20.0..=150.0).suffix(" ms"));
                ui.end_row();
                ui.label("Tuner gate");
                ui.add(Slider::new(&mut cfg.tuner_gate_db, -90.0..=-20.0).suffix(" dBFS"));
                ui.end_row();
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        });

        ui.add_space(6.0);
        theme::section(ui, "Note end (note-off)", |ui| {
            ui.label(
                RichText::new(
                    "A note ends when its level drops the threshold below its peak. Soft, gradual muting \
                     needs a smaller drop; hard, fast muting works with a larger one. The offset shifts \
                     every detected end, if lengths are consistently long or short.",
                )
                .small()
                .color(theme::dim()),
            );
            egui::Grid::new("note_end").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Threshold");
                ui.add(Slider::new(&mut cfg.detector.release_db, 3.0..=40.0).step_by(0.5).suffix(" dB below peak"));
                ui.end_row();
                ui.label("End offset");
                ui.add(Slider::new(&mut cfg.detector.end_offset_ms, -60.0..=60.0).step_by(1.0).suffix(" ms")).on_hover_text("Negative = notes end earlier");
                ui.end_row();
                ui.label("Last note");
                let meter = ctx.engine.meter();
                let text = meter.last_note_ms.map_or("--".to_owned(), |ms| format!("{ms:.0}"));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).font(theme::mono(16.0)).color(theme::green()));
                    ui.label(RichText::new("MS").font(theme::mono(13.0)).color(theme::cyan()));
                    ui.label(RichText::new("(pluck and mute a note to check)").small().color(theme::dim()));
                });
                ui.end_row();
            });
        });

        if ctx.settings.get::<AnalysisConfig>(ANALYSIS) != cfg {
            ctx.settings.set(ANALYSIS, &cfg);
            ctx.engine.set_analysis(cfg);
        }
    }
}

fn spread(v: &[f32]) -> f32 {
    let lo = v.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if v.is_empty() { 0.0 } else { (hi - lo) / 2.0 }
}

pub struct TunerPage;

impl PreferencePage for TunerPage {
    fn id(&self) -> &'static str {
        TunerPrefs::ID
    }
    fn title(&self) -> &'static str {
        "Tuner"
    }
    fn icon(&self) -> &'static str {
        icons::GUITAR
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut PrefsCtx) {
        let mut p: TunerPrefs = ctx.settings.get(TunerPrefs::ID);
        egui::Grid::new("tuner_prefs").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Reference A4");
            ui.add(DragValue::new(&mut p.a4).range(415.0..=466.0).speed(0.1).suffix(" Hz"));
            ui.end_row();
            ui.label(RichText::new("In tune (green) within").color(theme::green()));
            ui.add(Slider::new(&mut p.in_tune_cents, 0.5..=10.0).suffix(" ¢"));
            ui.end_row();
            ui.label(RichText::new("Close (amber) within").color(theme::amber()));
            ui.add(Slider::new(&mut p.close_cents, p.in_tune_cents..=30.0).suffix(" ¢"));
            ui.end_row();
        });
        ctx.settings.set(TunerPrefs::ID, &p);
    }
}

pub struct ScoringPage;

impl PreferencePage for ScoringPage {
    fn id(&self) -> &'static str {
        SCORING
    }
    fn title(&self) -> &'static str {
        "Scoring"
    }
    fn icon(&self) -> &'static str {
        icons::TARGET
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut PrefsCtx) {
        let mut t: ScoringThresholds = ctx.settings.get(SCORING);
        ui.label(RichText::new("Applies to new sessions.").small().color(theme::dim()));
        egui::Grid::new("scoring").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(RichText::new("Good timing within").color(theme::green()));
            ui.add(Slider::new(&mut t.good_ms, 2.0..=40.0).suffix(" ms"));
            ui.end_row();
            ui.label(RichText::new("OK timing within").color(theme::amber()));
            ui.add(Slider::new(&mut t.ok_ms, t.good_ms..=80.0).suffix(" ms"));
            ui.end_row();
            ui.label("Matching window");
            ui.add(Slider::new(&mut t.window_ms, 40.0..=300.0).suffix(" ms")).on_hover_text("Notes further than this from any target count as extra notes");
            ui.end_row();
            ui.label("Score note length");
            ui.checkbox(&mut t.score_length, "");
            ui.end_row();
            ui.label(RichText::new("Good length within").color(theme::green()));
            let mut g = t.length_good * 100.0;
            ui.add_enabled(t.score_length, Slider::new(&mut g, 5.0..=80.0).suffix(" %"));
            t.length_good = g / 100.0;
            ui.end_row();
            ui.label(RichText::new("OK length within").color(theme::amber()));
            let mut o = t.length_ok * 100.0;
            ui.add_enabled(t.score_length, Slider::new(&mut o, g..=150.0).suffix(" %"));
            t.length_ok = o / 100.0;
            ui.end_row();
        });
        ctx.settings.set(SCORING, &t);
    }
}

pub struct DisplayPage;

type ColorField = fn(&mut Palette) -> &mut Rgb;

const COLOR_FIELDS: [(&str, ColorField); 12] = [
    ("Background", |p| &mut p.bg),
    ("Panels", |p| &mut p.panel),
    ("Raised areas", |p| &mut p.raised),
    ("Structure lines", |p| &mut p.line),
    ("Grid lines", |p| &mut p.grid),
    ("Secondary text", |p| &mut p.dim),
    ("Text", |p| &mut p.text),
    ("Normal / good", |p| &mut p.green),
    ("Caution", |p| &mut p.amber),
    ("Warning", |p| &mut p.red),
    ("Selectable / units", |p| &mut p.cyan),
    ("Targets", |p| &mut p.magenta),
];

impl PreferencePage for DisplayPage {
    fn id(&self) -> &'static str {
        ThemePrefs::ID
    }
    fn title(&self) -> &'static str {
        "Display"
    }
    fn icon(&self) -> &'static str {
        icons::MONITOR
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut PrefsCtx) {
        let mut t: ThemePrefs = ctx.settings.get(ThemePrefs::ID);
        theme::section(ui, "Size", |ui| {
            egui::Grid::new("display_size").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                // Buttons rather than a slider: zooming moves the control under the pointer.
                ui.label("UI zoom");
                ui.horizontal(|ui| {
                    let pct = (t.zoom * 100.0).round() as i32;
                    if ui.add_enabled(pct > 50, egui::Button::new(icons::MINUS)).on_hover_text("−5%").clicked() {
                        t.zoom = ((pct - 5).max(50)) as f32 / 100.0;
                    }
                    ui.label(RichText::new(format!("{pct:>3}%")).font(theme::mono(16.0)).color(theme::green()));
                    if ui.add_enabled(pct < 250, egui::Button::new(icons::PLUS)).on_hover_text("+5%").clicked() {
                        t.zoom = ((pct + 5).min(250)) as f32 / 100.0;
                    }
                    if ui.add_enabled(pct != 100, egui::Button::new("100%")).clicked() {
                        t.zoom = 1.0;
                    }
                });
                ui.end_row();
                ui.label("");
                ui.label(RichText::new("Also: Cmd/Ctrl + and − to zoom, Cmd/Ctrl 0 to reset").small().color(theme::dim()));
                ui.end_row();
                ui.label("Text size");
                ui.horizontal(|ui| {
                    let pct = (t.text_scale * 100.0).round() as i32;
                    if ui.add_enabled(pct > 70, egui::Button::new(icons::MINUS)).on_hover_text("−5%").clicked() {
                        t.text_scale = ((pct - 5).max(70)) as f32 / 100.0;
                    }
                    ui.label(RichText::new(format!("{pct:>3}%")).font(theme::mono(16.0)).color(theme::green()));
                    if ui.add_enabled(pct < 160, egui::Button::new(icons::PLUS)).on_hover_text("+5%").clicked() {
                        t.text_scale = ((pct + 5).min(160)) as f32 / 100.0;
                    }
                    if ui.add_enabled(pct != 100, egui::Button::new("100%")).clicked() {
                        t.text_scale = 1.0;
                    }
                });
                ui.end_row();
                ui.label("Font");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut t.mono_ui, true, "Monospaced (ECAM)");
                    ui.radio_value(&mut t.mono_ui, false, "Proportional");
                });
                ui.end_row();
            });
        });
        ui.add_space(6.0);
        theme::section(ui, "Theme", |ui| {
            ui.horizontal_wrapped(|ui| {
                for (id, name, _) in PRESETS {
                    if ui.selectable_label(t.preset == id, name).clicked() {
                        t.preset = id.to_owned();
                    }
                }
                ui.selectable_value(&mut t.preset, "custom".to_owned(), "Custom");
            });
            ui.add_space(4.0);
            let mut palette = t.palette();
            let before = palette;
            egui::Grid::new("display_colors").num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
                for (i, (label, field)) in COLOR_FIELDS.iter().enumerate() {
                    let rgb = field(&mut palette);
                    let mut c = egui::Color32::from_rgb(rgb.0[0], rgb.0[1], rgb.0[2]);
                    egui::widgets::color_picker::color_edit_button_srgba(ui, &mut c, egui::widgets::color_picker::Alpha::Opaque);
                    *rgb = Rgb([c.r(), c.g(), c.b()]);
                    ui.label(*label);
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            if palette != before {
                // Editing a colour turns the current theme into the custom one.
                t.custom = palette;
                t.preset = "custom".into();
            }
            if t.preset != "custom" {
                ui.label(RichText::new("Changing a colour copies this theme into Custom.").small().color(theme::dim()));
            }
        });
        ctx.settings.set(ThemePrefs::ID, &t);
    }
}
