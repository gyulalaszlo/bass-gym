//! The preset bar: the selector, Save / New / Revert / Defaults, the more menu, and the
//! New / Rename / Delete dialogs. A tier-2 component: the open dialog is its state, and
//! every preset operation is an [`Emission`] its owner carries out on the store.

use bg_core::preset::StoredPreset;
use bg_ui::egui::{self, Color32, RichText, Ui};
use bg_ui::icons;
use bg_ui::kit::{Component, Emit};
use bg_ui::theme::Tokens;
use bg_ui::widgets::button::{button, selectable};
use bg_ui::widgets::controls::dropdown_sized;
use uuid::Uuid;

pub struct PresetBar;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum State {
    #[default]
    Closed,
    New(String),
    Rename(String),
    Delete,
}

pub struct Props<'a> {
    /// The selected preset's name, "Default" for the factory settings.
    pub name: String,
    pub current: Option<Uuid>,
    pub presets: &'a [StoredPreset],
    /// The working settings differ from the selected preset.
    pub dirty: bool,
    /// A session is running: presets can't be switched.
    pub locked: bool,
    /// How the last operation went: the text, and whether it worked.
    pub message: Option<&'a (String, bool)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Open(State),
    Name(String),
    Close,
    /// Something to do, closing the dialog.
    Do(Emission),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Emission {
    /// Load a preset (`None`: the factory default).
    Pick(Option<Uuid>),
    Save,
    SaveAs(String),
    Rename(String),
    Delete,
    Revert,
    Defaults,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: Color32,
    pub dirty: Color32,
    pub ok: Color32,
    pub error: Color32,
    pub danger: Color32,
    pub dialog_width: f32,
    /// The selector's narrowest, and its guess of the buttons' width before measuring.
    pub min_selector: f32,
    pub buttons_guess: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            name: t.cyan(),
            dirty: t.amber(),
            ok: t.green(),
            error: t.red(),
            danger: t.red(),
            dialog_width: 340.0,
            min_selector: 140.0,
            buttons_guess: 560.0,
        }
    }
}

impl Component for PresetBar {
    type State = State;
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Msg = Msg;
    type Emission = Emission;

    fn view(state: &State, p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
        let do_ = |e| Msg::Do(e);
        // The selector takes the space the buttons leave, measured on the previous frame
        // (a layout measurement, kept in egui's memory), so the buttons stay at the right.
        let buttons_id = ui.id().with("preset_buttons_width");
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!p.locked, |ui| {
                let dirty_mark = if p.dirty { "  •" } else { "" };
                let label = format!("{}  {}{dirty_mark}", icons::BOOKMARK_SIMPLE, p.name);
                let text =
                    RichText::new(label).color(if p.dirty { theme.dirty } else { theme.name });
                let buttons_w: f32 = ui
                    .data(|d| d.get_temp(buttons_id))
                    .unwrap_or(theme.buttons_guess);
                let width = (ui.available_width() - buttons_w - ui.spacing().item_spacing.x)
                    .max(theme.min_selector);
                dropdown_sized(ui, "preset_select", text, width, |ui| {
                    if ui.add(selectable(p.current.is_none(), "Default")).clicked() {
                        out.send(do_(Emission::Pick(None)));
                    }
                    for preset in p.presets {
                        let selected = p.current == Some(preset.envelope.id);
                        match &preset.incompatible {
                            None => {
                                if ui
                                    .add(selectable(selected, &preset.envelope.name))
                                    .clicked()
                                {
                                    out.send(do_(Emission::Pick(Some(preset.envelope.id))));
                                }
                            }
                            Some(why) => {
                                let name = format!("{} {}", icons::WARNING, preset.envelope.name);
                                ui.add_enabled(false, selectable(false, name))
                                    .on_disabled_hover_text(why);
                            }
                        }
                    }
                });
                let buttons = ui.horizontal(|ui| buttons(p, theme, ui, out));
                let w = buttons.response.rect.width();
                if (w - buttons_w).abs() > 0.5 {
                    ui.data_mut(|d| d.insert_temp(buttons_id, w));
                    ui.ctx().request_repaint();
                }
            });
        });
        if let Some((msg, ok)) = p.message {
            let color = if *ok { theme.ok } else { theme.error };
            ui.label(RichText::new(msg).small().color(color));
        }
        dialog(state, p, theme, ui, out);
    }

    fn update(state: &mut State, _p: &Props, msg: Msg, out: &mut Emit<Emission>) {
        match msg {
            Msg::Open(s) => *state = s,
            Msg::Name(n) => {
                if let State::New(name) | State::Rename(name) = state {
                    *name = n;
                }
            }
            Msg::Close => *state = State::Closed,
            Msg::Do(e) => {
                *state = State::Closed;
                out.send(e);
            }
        }
    }
}

fn buttons(p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    let can_save = p.dirty && p.current.is_some();
    if ui
        .add_enabled(can_save, button(format!("{} Save", icons::FLOPPY_DISK)))
        .on_hover_text("Overwrite the selected preset")
        .clicked()
    {
        out.send(Msg::Do(Emission::Save));
    }
    if ui
        .add(button(format!("{} New", icons::PLUS)))
        .on_hover_text("Save the current settings as a new preset")
        .clicked()
    {
        out.send(Msg::Open(State::New(String::new())));
    }
    if ui
        .add_enabled(
            p.dirty,
            button(format!("{} Revert", icons::ARROW_COUNTER_CLOCKWISE)),
        )
        .on_hover_text("Discard unsaved changes")
        .clicked()
    {
        out.send(Msg::Do(Emission::Revert));
    }
    if ui
        .add(button(format!("{} Defaults", icons::ARROW_U_UP_LEFT)))
        .on_hover_text("Reset the settings to the factory defaults (keeps the preset selected)")
        .clicked()
    {
        out.send(Msg::Do(Emission::Defaults));
    }
    ui.add_enabled_ui(p.current.is_some(), |ui| {
        let more = ui
            .add(button(icons::DOTS_THREE))
            .on_hover_text("More preset actions");
        egui::Popup::menu(&more).show(|ui| {
            let wide = [ui.available_width(), 0.0];
            if ui
                .add(button(format!("{} Rename…", icons::PENCIL_SIMPLE)).min_size(wide))
                .clicked()
            {
                out.send(Msg::Open(State::Rename(p.name.clone())));
            }
            if ui
                .add(
                    button(format!("{} Delete…", icons::TRASH))
                        .color(theme.danger)
                        .min_size(wide),
                )
                .clicked()
            {
                out.send(Msg::Open(State::Delete));
            }
        });
    });
}

fn dialog(state: &State, p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    if *state == State::Closed {
        return;
    }
    egui::Modal::new(egui::Id::new("preset_dialog")).show(ui.ctx(), |ui| {
        ui.set_width(theme.dialog_width);
        match state {
            State::New(name) | State::Rename(name) => {
                let is_new = matches!(state, State::New(_));
                ui.heading(if is_new {
                    "New preset"
                } else {
                    "Rename preset"
                });
                let mut edit = name.clone();
                let resp = ui.text_edit_singleline(&mut edit);
                resp.request_focus();
                if edit != *name {
                    out.send(Msg::Name(edit.clone()));
                }
                let valid = !edit.trim().is_empty();
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid, button("Save")).clicked() || (enter && valid) {
                        let name = edit.trim().to_owned();
                        out.send(Msg::Do(if is_new {
                            Emission::SaveAs(name)
                        } else {
                            Emission::Rename(name)
                        }));
                    }
                    if ui.add(button("Cancel")).clicked() {
                        out.send(Msg::Close);
                    }
                });
            }
            State::Delete => {
                ui.heading("Delete preset");
                ui.label(format!("Delete “{}”? This can't be undone.", p.name));
                ui.horizontal(|ui| {
                    if ui.add(button("Delete").color(theme.danger)).clicked() {
                        out.send(Msg::Do(Emission::Delete));
                    }
                    if ui.add(button("Cancel")).clicked() {
                        out.send(Msg::Close);
                    }
                });
            }
            State::Closed => {}
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            out.send(Msg::Close);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(state: &mut State, msg: Msg) -> Vec<Emission> {
        let p = Props {
            name: "Default".into(),
            current: None,
            presets: &[],
            dirty: false,
            locked: false,
            message: None,
        };
        let mut out = Emit::new();
        PresetBar::update(state, &p, msg, &mut out);
        out.into_vec()
    }

    #[test]
    fn a_dialog_keeps_its_name_and_closes_when_done() {
        let mut s = State::default();
        send(&mut s, Msg::Open(State::New(String::new())));
        send(&mut s, Msg::Name("slow".into()));
        assert_eq!(s, State::New("slow".into()));
        let out = send(&mut s, Msg::Do(Emission::SaveAs("slow".into())));
        assert_eq!(out, vec![Emission::SaveAs("slow".into())]);
        assert_eq!(s, State::Closed);
    }
}
