//! The Display page: size, the exercise view, buttons and the colour theme.

use bg_ui::egui::{self, RichText, Ui};
use bg_ui::kit::{self, Emit, ThemeCache, View};
use bg_ui::theme::{PRESETS, Palette, Rgb, ThemePrefs, section};
use bg_ui::view::{SCORE_SIZE_RANGE, ViewPrefs, ZOOM_STEPS};
use bg_ui::widgets::button::{button, selectable, selectable_value};
use bg_ui::widgets::controls::{checkbox, dropdown_value, number_field, slider_field};
use bg_ui::{PreferencePage, PrefsCtx, icons};

use super::PageTheme;

#[derive(Default)]
pub struct DisplayPage {
    theme: ThemeCache<PageTheme>,
}

type ColorField = fn(&mut Palette) -> &mut Rgb;

const COLOR_FIELDS: [(&str, ColorField); 13] = [
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
    ("Button base", |p| &mut p.button_base),
];

/// UI zoom, text size and font.
struct Size;

impl View for Size {
    type Props<'a> = ThemePrefs;
    type Theme = PageTheme;
    type Emission = ThemePrefs;

    fn view(p: &ThemePrefs, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<ThemePrefs>) {
        let edit = kit::edited(p, |t| {
            section(ui, "Size", |ui| {
                egui::Grid::new("display_size")
                    .num_columns(2)
                    .spacing(theme.grid_spacing)
                    .show(ui, |ui| {
                        // Buttons rather than a slider: zooming moves the control under the pointer.
                        ui.label("UI zoom");
                        ui.horizontal(|ui| {
                            let pct = (t.zoom * 100.0).round() as i32;
                            if ui
                                .add_enabled(pct > 50, button(icons::MINUS))
                                .on_hover_text("−5%")
                                .clicked()
                            {
                                t.zoom = ((pct - 5).max(50)) as f32 / 100.0;
                            }
                            ui.label(
                                RichText::new(format!("{pct:>3}%"))
                                    .font(theme.value_font.clone())
                                    .color(theme.ok),
                            );
                            if ui
                                .add_enabled(pct < 250, button(icons::PLUS))
                                .on_hover_text("+5%")
                                .clicked()
                            {
                                t.zoom = ((pct + 5).min(250)) as f32 / 100.0;
                            }
                            if ui.add_enabled(pct != 100, button("100%")).clicked() {
                                t.zoom = 1.0;
                            }
                        });
                        ui.end_row();
                        ui.label("");
                        ui.label(
                            RichText::new("Also: Cmd/Ctrl + and − to zoom, Cmd/Ctrl 0 to reset")
                                .small()
                                .color(theme.note),
                        );
                        ui.end_row();
                        ui.label("Text size");
                        ui.horizontal(|ui| {
                            let pct = (t.text_scale * 100.0).round() as i32;
                            if ui
                                .add_enabled(pct > 70, button(icons::MINUS))
                                .on_hover_text("−5%")
                                .clicked()
                            {
                                t.text_scale = ((pct - 5).max(70)) as f32 / 100.0;
                            }
                            ui.label(
                                RichText::new(format!("{pct:>3}%"))
                                    .font(theme.value_font.clone())
                                    .color(theme.ok),
                            );
                            if ui
                                .add_enabled(pct < 160, button(icons::PLUS))
                                .on_hover_text("+5%")
                                .clicked()
                            {
                                t.text_scale = ((pct + 5).min(160)) as f32 / 100.0;
                            }
                            if ui.add_enabled(pct != 100, button("100%")).clicked() {
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
        });
        if let Some(p) = edit {
            out.send(p);
        }
    }
}

/// How the exercises show the grid and the score.
struct ExerciseView;

impl View for ExerciseView {
    type Props<'a> = ViewPrefs;
    type Theme = PageTheme;
    type Emission = ViewPrefs;

    fn view(p: &ViewPrefs, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<ViewPrefs>) {
        let edit = kit::edited(p, |v| {
            section(ui, "Exercise view", |ui| {
                egui::Grid::new("display_view")
                    .num_columns(2)
                    .spacing(theme.grid_spacing)
                    .show(ui, |ui| {
                        ui.label("Beat grid zoom");
                        dropdown_value(
                            ui,
                            "view_zoom",
                            &mut v.grid_zoom,
                            0..ZOOM_STEPS.len(),
                            |i| format!("×{}", ZOOM_STEPS[*i]),
                        );
                        ui.end_row();
                        ui.label("Played notes");
                        checkbox(ui, &mut v.show_score, "Show as a score above the grid");
                        ui.end_row();
                        ui.label("Score size");
                        ui.add_enabled(
                            v.show_score,
                            slider_field(&mut v.score_size, SCORE_SIZE_RANGE).suffix("pt"),
                        )
                        .on_hover_text("Staff line spacing of the live score above the grid");
                        ui.end_row();
                        ui.label("Results score size");
                        ui.add(
                            slider_field(&mut v.results_score_size, SCORE_SIZE_RANGE).suffix("pt"),
                        )
                        .on_hover_text(
                            "Staff line spacing of the score in Results › Score + Audio",
                        );
                        ui.end_row();
                        ui.label("Performance indicator range");
                        let unit = if v.indicator_bars == 1 { "bar" } else { "bars" };
                        ui.add(
                            number_field(&mut v.indicator_bars)
                                .range(1..=8)
                                .suffix(format!("last {unit}")),
                        );
                        ui.end_row();
                    });
            });
        });
        if let Some(p) = edit {
            out.send(p);
        }
    }
}

/// Button rounding and fill, with a preview.
struct Buttons;

impl View for Buttons {
    type Props<'a> = ThemePrefs;
    type Theme = PageTheme;
    type Emission = ThemePrefs;

    fn view(p: &ThemePrefs, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<ThemePrefs>) {
        let edit = kit::edited(p, |t| {
            section(ui, "Buttons", |ui| {
                egui::Grid::new("display_buttons").num_columns(2).spacing(theme.grid_spacing).show(ui, |ui| {
                    ui.label("Rounding");
                    ui.add(slider_field(&mut t.button_rounding, 0.0..=16.0).step_by(1.0).suffix("px"));
                    ui.end_row();
                    ui.label("Base strength");
                    let mut pct = (t.button_tint * 100.0).round();
                    if ui.add(slider_field(&mut pct, 0.0..=100.0).step_by(1.0).suffix("%")).changed() {
                        t.button_tint = pct / 100.0;
                    }
                    ui.end_row();
                    ui.label("");
                    ui.label(RichText::new("How much of each button's colour mixes into the \"Button base\" colour for its resting fill").small().color(theme.note));
                    ui.end_row();
                    ui.label("Preview");
                    ui.horizontal_wrapped(|ui| {
                        ui.add(button(format!("{} Button", icons::CURSOR_CLICK)));
                        ui.add(button(format!("{} Start", icons::PLAY)).color(theme.ok));
                        ui.add(button(format!("{} Stop", icons::STOP)).color(theme.warning));
                        ui.add(button(format!("{} Delete", icons::TRASH)).color(theme.error));
                        ui.add_enabled(false, button("Disabled"));
                    });
                    ui.end_row();
                });
            });
        });
        if let Some(p) = edit {
            out.send(p);
        }
    }
}

/// The colour theme: a preset, or custom colours.
struct Colours;

impl View for Colours {
    type Props<'a> = ThemePrefs;
    type Theme = PageTheme;
    type Emission = ThemePrefs;

    fn view(p: &ThemePrefs, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<ThemePrefs>) {
        let edit = kit::edited(p, |t| {
            section(ui, "Theme", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (id, name, _) in PRESETS {
                        if ui.add(selectable(t.preset == id, name)).clicked() {
                            t.preset = id.to_owned();
                        }
                    }
                    selectable_value(ui, &mut t.preset, "custom".to_owned(), "Custom");
                });
                ui.add_space(theme.gap);
                let mut palette = t.palette();
                let before = palette;
                egui::Grid::new("display_colors")
                    .num_columns(4)
                    .spacing(theme.tight_grid_spacing)
                    .show(ui, |ui| {
                        for (i, (label, field)) in COLOR_FIELDS.iter().enumerate() {
                            let rgb = field(&mut palette);
                            let mut c = egui::Color32::from_rgb(rgb.0[0], rgb.0[1], rgb.0[2]);
                            egui::widgets::color_picker::color_edit_button_srgba(
                                ui,
                                &mut c,
                                egui::widgets::color_picker::Alpha::Opaque,
                            );
                            *rgb = Rgb([c.r(), c.g(), c.b()]);
                            ui.label(*label);
                            if i % 2 == 1 || i + 1 == COLOR_FIELDS.len() {
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
                    ui.label(
                        RichText::new("Changing a colour copies this theme into Custom.")
                            .small()
                            .color(theme.note),
                    );
                }
            });
        });
        if let Some(p) = edit {
            out.send(p);
        }
    }
}

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

    fn ui(&mut self, ui: &mut Ui, ctx: &mut PrefsCtx) {
        let theme = self.theme.get();
        let t: ThemePrefs = ctx.settings.get(ThemePrefs::ID);
        let v: ViewPrefs = ctx.settings.get(ViewPrefs::ID);
        let mut themes = kit::show::<Size>(ui, "size", &t, &theme);
        ui.add_space(theme.section_gap);
        let views = kit::show::<ExerciseView>(ui, "view", &v, &theme);
        ui.add_space(theme.section_gap);
        themes.extend(kit::show::<Buttons>(ui, "buttons", &t, &theme));
        ui.add_space(theme.section_gap);
        themes.extend(kit::show::<Colours>(ui, "colours", &t, &theme));
        // One edit per frame in practice; the last wins.
        if let Some(t) = themes.pop() {
            ctx.settings.set(ThemePrefs::ID, &t);
        }
        if let Some(v) = views.into_iter().last() {
            ctx.settings.set(ViewPrefs::ID, &v);
        }
    }
}
