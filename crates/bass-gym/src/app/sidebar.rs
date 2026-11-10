//! The sidebar: the exercise list, the preferences button, the audio status and the
//! monitoring controls. A tier-1 view.

use bg_audio::{EngineStatus, SoundConfig};
use bg_ui::egui::{self, Color32, FontId, RichText, Ui, Vec2, vec2};
use bg_ui::icons;
use bg_ui::kit::{self, Emit, View};
use bg_ui::theme::{Tokens, title_label};
use bg_ui::widgets::button::{button, selectable};

pub struct Sidebar;

pub struct Item {
    pub icon: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

pub struct Props<'a> {
    pub exercises: &'a [Item],
    pub selected: usize,
    /// A session runs: the other exercises can't be picked.
    pub running: bool,
    pub status: &'a EngineStatus,
    pub xruns: u32,
    pub sounds: &'a SoundConfig,
}

pub enum Emission {
    Select(usize),
    OpenPreferences,
    /// Monitoring was changed here.
    Sounds(SoundConfig),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub logo: Color32,
    pub logo_size: f32,
    pub title_font: FontId,
    pub title: Color32,
    pub ok: Color32,
    pub warning: Color32,
    /// A toggle that's off, and one that's on.
    pub toggle: Color32,
    pub toggle_on: Color32,
    pub item_height: f32,
    pub button_height: f32,
    /// Above the title, below it, below "Exercises", at the bottom, above the status and
    /// above the monitoring controls.
    pub gaps: [f32; 6],
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            logo: t.green(),
            logo_size: 22.0,
            title_font: t.mono(20.0),
            title: t.text(),
            ok: t.green(),
            warning: t.amber(),
            toggle: t.cyan(),
            toggle_on: t.green(),
            item_height: 34.0,
            button_height: 32.0,
            gaps: [12.0, 14.0, 4.0, 10.0, 4.0, 6.0],
        }
    }
}

impl View for Sidebar {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Emission;

    fn view(p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Emission>) {
        let [top, title_gap, list_gap, bottom, status_gap, monitor_gap] = theme.gaps;
        ui.add_space(top);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(icons::WAVE_SINE)
                    .size(theme.logo_size)
                    .color(theme.logo),
            );
            ui.label(
                RichText::new("BASS GYM")
                    .font(theme.title_font.clone())
                    .color(theme.title),
            );
        });
        ui.add_space(title_gap);
        title_label(ui, "Exercises");
        ui.add_space(list_gap);
        for (i, e) in p.exercises.iter().enumerate() {
            let selected = i == p.selected;
            let text = format!("{}  {}", e.icon, e.name.to_uppercase());
            let size: Vec2 = vec2(ui.available_width(), theme.item_height);
            let resp = ui.add_enabled(
                !p.running || selected,
                selectable(selected, text).min_size(size),
            );
            if resp.on_hover_text(e.description).clicked() {
                out.send(Emission::Select(i));
            }
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(bottom);
            let shortcut = if cfg!(target_os = "macos") {
                "⌘,"
            } else {
                "Ctrl+,"
            };
            if ui
                .add(
                    button(format!("{}  PREFERENCES", icons::GEAR))
                        .min_size(vec2(ui.available_width(), theme.button_height)),
                )
                .on_hover_text(format!("Open the preferences ({shortcut})"))
                .clicked()
            {
                out.send(Emission::OpenPreferences);
            }
            ui.add_space(status_gap);
            let st = p.status;
            let dot = |ok: bool| if ok { theme.ok } else { theme.warning };
            ui.label(
                RichText::new(format!("OUT {}", st.output.as_deref().unwrap_or("—")))
                    .small()
                    .color(dot(st.output.is_some())),
            );
            ui.label(
                RichText::new(format!("IN  {}", st.input.as_deref().unwrap_or("—")))
                    .small()
                    .color(dot(st.input.is_some())),
            );
            if p.xruns > 0 {
                ui.label(
                    RichText::new(format!("{} {} XRUNS", icons::WARNING, p.xruns))
                        .small()
                        .color(theme.warning),
                )
                .on_hover_text(
                    "Audio buffer under/overruns. Try a larger buffer size in Preferences › Audio.",
                );
            }
            ui.add_space(monitor_gap);
            // Input monitoring: hear the bass through the output, with its round-trip delay.
            let edit = kit::edited(p.sounds, |sounds| monitoring(ui, theme, sounds));
            if let Some(s) = edit {
                out.send(Emission::Sounds(s));
            }
        });
    }
}

fn monitoring(ui: &mut Ui, theme: &Theme, sounds: &mut SoundConfig) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut sounds.monitor_gain, 0.0..=2.0).show_value(false))
            .on_hover_text("Monitor volume");
    });
    let toggle_color = |on: bool| if on { theme.toggle_on } else { theme.toggle };
    ui.horizontal(|ui| {
        let on = sounds.monitor;
        let label = format!("{}  MONITOR {}", icons::EAR, if on { "ON" } else { "OFF" });
        // M toggles it too, unless a text field has the keyboard.
        let key = ui.input(|i| i.modifiers.is_none() && i.key_pressed(egui::Key::M))
            && !ui.ctx().egui_wants_keyboard_input();
        if ui
            .add(selectable(on, label).color(toggle_color(on)))
            .on_hover_text("Mix the bass input into the output (M)")
            .clicked()
            || key
        {
            sounds.monitor = !on;
        }
        // Quiet practice: a low cut on the monitored bass (cutoff in Preferences › Sounds).
        let quiet = sounds.quiet;
        let tip = format!(
            "Quiet practice {}: cut the monitored bass below {:.0} Hz, so it carries less through walls and floors (frequency in Preferences › Sounds)",
            if quiet { "on" } else { "off" },
            sounds.quiet_cutoff,
        );
        if ui
            .add(selectable(quiet, icons::MOON).color(toggle_color(quiet)))
            .on_hover_text(tip)
            .clicked()
        {
            sounds.quiet = !quiet;
        }
    });
}
