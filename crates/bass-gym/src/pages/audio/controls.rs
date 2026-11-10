//! The controls the Audio page's sections share.

use bg_audio::{LOWPASS_OFF, SoundConfig};
use bg_sound::SoundRole;
use bg_ui::egui::{self, RichText, Slider};
use bg_ui::widgets::button::selectable_value;
use bg_ui::widgets::controls::dropdown;

use crate::pages::PageTheme;

pub fn device_combo(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Option<String>,
    list: &[bg_audio::DeviceInfo],
) {
    let text = match value {
        None => "System default".to_owned(),
        Some(v) => list
            .iter()
            .find(|d| &d.id == v)
            .map_or_else(|| "(unavailable device)".to_owned(), |d| d.name.clone()),
    };
    dropdown(ui, id, text, |ui| {
        selectable_value(ui, value, None, "System default");
        for d in list {
            selectable_value(
                ui,
                value,
                Some(d.id.clone()),
                format!("{}  ({} ch, {} Hz)", d.name, d.channels, d.default_rate),
            );
        }
    });
}

/// Low-pass cutoff for the current instrument, on a log scale; the top end is "off".
pub fn lowpass_slider(ui: &mut egui::Ui, theme: &PageTheme, sounds: &mut SoundConfig) {
    let hz = sounds.lowpass_setting(&sounds.instrument);
    let mut pos = (hz.clamp(400.0, LOWPASS_OFF) / 400.0).log2();
    let max = (LOWPASS_OFF / 400.0).log2();
    let fmt = |p: f64, _: std::ops::RangeInclusive<usize>| {
        let hz = 400.0 * 2f64.powf(p);
        if hz >= LOWPASS_OFF as f64 - 1.0 {
            "Open".to_owned()
        } else if hz >= 1000.0 {
            format!("{:.1} kHz", hz / 1000.0)
        } else {
            format!("{hz:.0} Hz")
        }
    };
    let resp = ui
        .horizontal(|ui| {
            let r = ui.add(Slider::new(&mut pos, 0.0..=max).show_value(false));
            ui.label(RichText::new(format!("{} low-pass", fmt(pos as f64, 0..=0))).color(theme.unit));
            r
        })
        .inner
        .on_hover_text("Darkens the instrument: a lower cutoff removes more of the bright overtones. Remembered per instrument.");
    if resp.changed() {
        sounds
            .instrument_lowpass
            .insert(sounds.instrument.clone(), 400.0 * 2f32.powf(pos));
    }
}

pub fn sound_combo(ui: &mut egui::Ui, id: &str, value: &mut String, role: SoundRole) {
    let text = bg_sound::source(value).map_or(value.clone(), |s| s.name().to_owned());
    dropdown(ui, id, text, |ui| {
        for s in bg_sound::sources_for(role) {
            selectable_value(ui, value, s.id().to_owned(), s.name());
        }
    });
}
