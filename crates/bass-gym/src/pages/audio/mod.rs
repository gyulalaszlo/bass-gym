//! The Audio page: devices, sounds, and session recording with the saved reports.

mod controls;
mod devices;
mod reports;
mod sounds;

use bg_audio::{AudioConfig, DeviceLists, SoundConfig};
use bg_ui::egui::Ui;
use bg_ui::icons;
use bg_ui::kit::{self, Emit, ThemeCache, View};
use bg_ui::recording::{MAX_MB_RANGE, RecordingPrefs};
use bg_ui::theme::section;
use bg_ui::widgets::button::button;
use bg_ui::widgets::controls::number_field;
use bg_ui::{PreferencePage, PrefsCtx};

use super::{AUDIO, PageTheme, SOUNDS};
use devices::Devices;
use reports::{Reports, ReportsView};
use sounds::Sounds;

#[derive(Default)]
pub struct AudioPage {
    /// Read from the engine when the page first shows, and on "Rescan".
    devices: Option<DeviceLists>,
    reports: Reports,
    theme: ThemeCache<PageTheme>,
}

/// The Session recording section: the buffer size, and the reports.
struct Recording;

struct RecordingProps<'a> {
    prefs: &'a RecordingPrefs,
    /// The input's sample rate, 0 when closed.
    input_rate: u32,
    reports: &'a Reports,
}

enum RecordingEdit {
    Prefs(RecordingPrefs),
    Reports(reports::Emission),
}

impl View for Recording {
    type Props<'a> = RecordingProps<'a>;
    type Theme = PageTheme;
    type Emission = RecordingEdit;

    fn view(p: &RecordingProps, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<RecordingEdit>) {
        section(ui, "Session recording", |ui| {
            let edit = kit::edited(p.prefs, |r| {
                ui.horizontal(|ui| {
                    ui.label("Keep up to");
                    ui.add(number_field(&mut r.max_mb).range(MAX_MB_RANGE).suffix("MB"));
                });
            });
            if p.input_rate != 0 {
                let minutes = edit.as_ref().unwrap_or(p.prefs).minutes(p.input_rate);
                ui.label(theme.note(format!(
                    "About {minutes:.0} minutes of audio. Past that, the oldest audio is overwritten."
                )));
            }
            if let Some(r) = edit {
                out.send(RecordingEdit::Prefs(r));
            }
            ui.add_space(theme.gap);
            let msgs = kit::show::<ReportsView>(ui, "reports", p.reports, theme);
            out.extend_map(msgs, RecordingEdit::Reports);
        });
    }
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

    fn ui(&mut self, ui: &mut Ui, ctx: &mut PrefsCtx) {
        let theme = self.theme.get();
        // Sync: what the page shows from outside.
        let devices = self
            .devices
            .get_or_insert_with(|| ctx.engine.devices())
            .clone();
        self.reports.sync();
        let cfg: AudioConfig = ctx.settings.get(AUDIO);
        let sounds: SoundConfig = ctx.settings.get(SOUNDS);
        let recording: RecordingPrefs = ctx.settings.get(RecordingPrefs::ID);
        let status = ctx.engine.status().clone();

        // View.
        let device_msgs = section(ui, "Devices", |ui| {
            let props = devices::Props {
                cfg: &cfg,
                devices: &devices,
                status: &status,
                xruns: ctx.engine.xruns(),
            };
            kit::show::<Devices>(ui, "devices", &props, &theme)
        });
        ui.add_space(theme.section_gap);
        let sound_msgs = kit::show::<Sounds>(ui, "sounds", &sounds, &theme);
        ui.add_space(theme.section_gap);
        let props = RecordingProps {
            prefs: &recording,
            input_rate: status.input_rate,
            reports: &self.reports,
        };
        let recording_msgs = kit::show::<Recording>(ui, "recording", &props, &theme);
        if let Some(dir) = crate::soundfonts::user_dir() {
            ui.horizontal_wrapped(|ui| {
                ui.label(theme.note(format!(
                    "More instruments: put SoundFont (.sf2) files in {} and restart.",
                    dir.display()
                )));
                if ui
                    .add(button(format!("{} Open folder", icons::FOLDER_OPEN)).small())
                    .clicked()
                {
                    let _ = std::fs::create_dir_all(&dir);
                    bg_ui::folder::open_folder(&dir);
                }
            });
        }

        // Update and effects: store what changed, and apply it.
        for m in device_msgs {
            match m {
                devices::Emission::Config(cfg) => {
                    ctx.settings.set(AUDIO, &cfg);
                    ctx.engine.set_config(cfg);
                }
                devices::Emission::Rescan => self.devices = None,
                devices::Emission::Reopen => ctx.engine.rebuild(),
            }
        }
        for s in sound_msgs {
            ctx.settings.set(SOUNDS, &s);
            ctx.engine.set_sounds(s);
        }
        for m in recording_msgs {
            match m {
                RecordingEdit::Prefs(r) => ctx.settings.set(RecordingPrefs::ID, &r),
                RecordingEdit::Reports(e) => self.reports.handle(e),
            }
        }
    }
}
