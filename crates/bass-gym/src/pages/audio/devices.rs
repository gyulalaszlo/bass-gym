//! The Devices section: input, channel, output, rate and buffer, and how the audio opened.

use bg_audio::{AudioConfig, DeviceLists, EngineStatus};
use bg_ui::egui::{RichText, Ui};
use bg_ui::icons;
use bg_ui::kit::{self, Emit, View};
use bg_ui::widgets::button::{button, selectable_value};
use bg_ui::widgets::controls::dropdown;

use super::controls::device_combo;
use crate::pages::{PageTheme, grid};

pub struct Devices;

pub struct Props<'a> {
    pub cfg: &'a AudioConfig,
    pub devices: &'a DeviceLists,
    pub status: &'a EngineStatus,
    /// Buffer under / overruns since the audio opened.
    pub xruns: u32,
}

pub enum Emission {
    Config(AudioConfig),
    Rescan,
    Reopen,
}

impl View for Devices {
    type Props<'a> = Props<'a>;
    type Theme = PageTheme;
    type Emission = Emission;

    fn view(p: &Props, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<Emission>) {
        let (devices, st) = (p.devices, p.status);
        let edit = kit::edited(p.cfg, |cfg| {
            grid("audio_devices", theme).show(ui, |ui| {
                ui.label(format!("{} Bass input", icons::MICROPHONE));
                device_combo(ui, "in_dev", &mut cfg.input_device, &devices.inputs);
                ui.end_row();

                ui.label("Input channel");
                let channels = devices
                    .inputs
                    .iter()
                    .find(|d| Some(&d.id) == cfg.input_device.as_ref())
                    .map(|d| d.channels)
                    .unwrap_or(st.input_channels.max(1));
                let text = cfg
                    .input_channel
                    .map_or("Mix all".to_owned(), |c| format!("Channel {}", c + 1));
                dropdown(ui, "in_ch", text, |ui| {
                    selectable_value(ui, &mut cfg.input_channel, None, "Mix all");
                    for c in 0..channels {
                        selectable_value(
                            ui,
                            &mut cfg.input_channel,
                            Some(c),
                            format!("Channel {}", c + 1),
                        );
                    }
                });
                ui.end_row();

                ui.label(format!("{} Output", icons::SPEAKER_HIGH));
                device_combo(ui, "out_dev", &mut cfg.output_device, &devices.outputs);
                ui.end_row();

                ui.label("Sample rate");
                let text = cfg
                    .sample_rate
                    .map_or("Device default".to_owned(), |r| format!("{r} Hz"));
                dropdown(ui, "rate", text, |ui| {
                    selectable_value(ui, &mut cfg.sample_rate, None, "Device default");
                    for r in [44_100, 48_000, 88_200, 96_000] {
                        selectable_value(ui, &mut cfg.sample_rate, Some(r), format!("{r} Hz"));
                    }
                });
                ui.end_row();

                ui.label("Buffer size");
                let text = cfg
                    .buffer_frames
                    .map_or("Device default".to_owned(), |f| format!("{f} frames"));
                dropdown(ui, "buf", text, |ui| {
                    selectable_value(ui, &mut cfg.buffer_frames, None, "Device default");
                    for f in [64, 128, 256, 512, 1024] {
                        selectable_value(
                            ui,
                            &mut cfg.buffer_frames,
                            Some(f),
                            format!("{f} frames"),
                        );
                    }
                });
                ui.end_row();
            });
        });
        if let Some(cfg) = edit {
            out.send(Emission::Config(cfg));
        }
        ui.horizontal(|ui| {
            if ui
                .add(button(format!(
                    "{} Rescan devices",
                    icons::ARROWS_CLOCKWISE
                )))
                .clicked()
            {
                out.send(Emission::Rescan);
            }
            if ui
                .add(button(format!("{} Reopen audio", icons::PLUGS)))
                .clicked()
            {
                out.send(Emission::Reopen);
            }
        });
        status_row(
            ui,
            theme,
            "Output",
            st.output.as_ref(),
            st.output_error.as_ref(),
            format!("{} Hz", st.output_rate),
        );
        status_row(
            ui,
            theme,
            "Input",
            st.input.as_ref(),
            st.input_error.as_ref(),
            format!("{} Hz, {} ch", st.input_rate, st.input_channels),
        );
        if st.shared_device {
            ui.label(theme.note("Input and output share a device: both use the output's sample rate and buffer size."));
        }
        if p.xruns > 0 {
            ui.label(
                RichText::new(format!(
                    "{} {} buffer under/overruns since the audio was opened — try a larger buffer size.",
                    icons::WARNING,
                    p.xruns
                ))
                .small()
                .color(theme.warning),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(500));
        }
        if st.input_rate != 0 && st.output_rate != 0 && st.input_rate != st.output_rate {
            ui.label(
                RichText::new("Input and output run at different sample rates; timing still works, but a shared rate is more stable.")
                    .small()
                    .color(theme.warning),
            );
        }
    }
}

fn status_row(
    ui: &mut Ui,
    theme: &PageTheme,
    label: &str,
    ok: Option<&String>,
    err: Option<&String>,
    detail: String,
) {
    ui.horizontal(|ui| match (ok, err) {
        (Some(name), _) => {
            ui.label(RichText::new(icons::CHECK_CIRCLE).color(theme.ok));
            ui.label(format!("{label}: {name}"));
            ui.label(theme.note(detail));
        }
        (None, Some(e)) => {
            ui.label(RichText::new(icons::WARNING).color(theme.warning));
            ui.label(RichText::new(format!("{label}: {e}")).color(theme.warning));
        }
        (None, None) => {
            ui.label(RichText::new(format!("{label}: not open")).color(theme.note));
        }
    });
}
