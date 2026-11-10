//! The Sounds section: what plays the click, groove and instrument, volume and monitoring.

use bg_audio::{QUIET_CUTOFF_RANGE, SoundConfig};
use bg_sound::SoundRole;
use bg_ui::egui::{self, Ui};
use bg_ui::icons;
use bg_ui::kit::{self, Emit, View};
use bg_ui::theme::section;
use bg_ui::widgets::controls::{checkbox, slider_field};

use super::controls::{lowpass_slider, sound_combo};
use crate::pages::PageTheme;

pub struct Sounds;

impl View for Sounds {
    type Props<'a> = SoundConfig;
    type Theme = PageTheme;
    type Emission = SoundConfig;

    fn view(p: &SoundConfig, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<SoundConfig>) {
        let edit = kit::edited(p, |sounds| {
            section(ui, "Sounds", |ui| {
                egui::Grid::new("sounds")
                    .num_columns(2)
                    .spacing(theme.grid_spacing)
                    .show(ui, |ui| {
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
                        lowpass_slider(ui, theme, sounds);
                        ui.end_row();
                        ui.label("Master volume");
                        ui.add(slider_field(&mut sounds.master, 0.0..=1.0));
                        ui.end_row();
                        ui.label(format!("{} Monitor input", icons::EAR));
                        ui.horizontal(|ui| {
                            checkbox(ui, &mut sounds.monitor, "");
                            ui.add_enabled(
                                sounds.monitor,
                                slider_field(&mut sounds.monitor_gain, 0.0..=2.0),
                            );
                        });
                        ui.end_row();
                        ui.label(format!("{} Quiet practice", icons::MOON))
                            .on_hover_text("A low cut on the monitored bass, so it carries less through walls and floors");
                        ui.horizontal(|ui| {
                            checkbox(ui, &mut sounds.quiet, "");
                            ui.add_enabled(
                                sounds.quiet,
                                slider_field(&mut sounds.quiet_cutoff, QUIET_CUTOFF_RANGE)
                                    .step_by(5.0)
                                    .suffix("Hz"),
                            )
                            .on_hover_text("Low-cut frequency: lower keeps more of the bass, higher is quieter");
                        });
                        ui.end_row();
                    });
            });
        });
        if let Some(s) = edit {
            out.send(s);
        }
    }
}
