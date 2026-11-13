//! The preset bar: the folder browser and preset selector, Save / New / Edit / Revert /
//! Defaults, the more menu, and the New / Edit / Delete dialogs. A tier-2 component: the open dialog is its state, and
//! every preset operation is an [`Emission`] its owner carries out on the store.

use std::path::PathBuf;

use bg_core::preset::StoredPreset;
use bg_ui::egui::{self, Color32, RichText, Ui};
use bg_ui::icons;
use bg_ui::kit::{self, Component, Emit};
use bg_ui::theme::Tokens;
use bg_ui::widgets::button::{button, selectable};
use bg_ui::widgets::controls::dropdown_sized;
use uuid::Uuid;

use super::browser::{self, Browser};

pub struct PresetBar;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum State {
    #[default]
    Closed,
    New(String),
    Edit(Draft),
    Delete,
}

/// The Edit dialog's fields; tags as typed, comma separated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub name: String,
    pub description: String,
    pub tags: String,
}

pub struct Props<'a> {
    /// The selected preset's name, "Default" for the factory settings.
    pub name: String,
    pub current: Option<Uuid>,
    pub presets: &'a [StoredPreset],
    /// The preset dirs' names, every folder in them, and the one shown.
    pub roots: &'a [String],
    pub folders: &'a [(usize, PathBuf)],
    pub folder: &'a (usize, PathBuf),
    /// The selected preset's description and tags.
    pub description: &'a str,
    pub tags: &'a [String],
    /// What changed since the last load or save, one line each.
    pub changes: &'a [String],
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
    /// Show a folder: `(preset dir, relative path)`.
    Folder(usize, PathBuf),
    Edit {
        name: String,
        description: String,
        tags: Vec<String>,
    },
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
    pub edit_width: f32,
    pub dialog_margin: i8,
    pub field_gap: f32,
    /// The selector's narrowest, and its guess of the buttons' width before measuring.
    pub min_selector: f32,
    pub buttons_guess: f32,
    /// The browser's share of the selector width.
    pub browser_share: f32,
    pub changes_height: f32,
    pub browser: browser::Theme,
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
            edit_width: 520.0,
            dialog_margin: 20,
            field_gap: 8.0,
            min_selector: 140.0,
            buttons_guess: 640.0,
            browser_share: 0.4,
            changes_height: 160.0,
            browser: browser::Theme::default(),
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
                let width = (ui.available_width() - buttons_w - 2.0 * ui.spacing().item_spacing.x)
                    .max(2.0 * theme.min_selector);
                let browser_w = (width * theme.browser_share).max(theme.min_selector);
                let props = browser::Props {
                    roots: p.roots,
                    folders: p.folders,
                    selected: p.folder,
                    width: browser_w,
                };
                for (root, folder) in kit::show::<Browser>(ui, "browser", &props, &theme.browser) {
                    out.send(do_(Emission::Folder(root, folder)));
                }
                let in_folder = p
                    .presets
                    .iter()
                    .filter(|x| (x.root, &x.folder) == (p.folder.0, &p.folder.1));
                dropdown_sized(ui, "preset_select", text, width - browser_w, |ui| {
                    if ui.add(selectable(p.current.is_none(), "Default")).clicked() {
                        out.send(do_(Emission::Pick(None)));
                    }
                    for preset in in_folder {
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
                if let State::New(name) = state {
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
            p.current.is_some(),
            button(format!("{} Edit", icons::PENCIL_SIMPLE)),
        )
        .on_hover_text("Edit the name, description and tags, and see what changed")
        .clicked()
    {
        out.send(Msg::Open(State::Edit(Draft {
            name: p.name.clone(),
            description: p.description.to_owned(),
            tags: p.tags.join(", "),
        })));
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
    let frame = egui::Frame::popup(&ui.ctx().global_style()).inner_margin(theme.dialog_margin);
    let modal = egui::Modal::new(egui::Id::new("preset_dialog")).frame(frame);
    modal.show(ui.ctx(), |ui| {
        let wide = matches!(state, State::Edit(_));
        ui.set_width(if wide {
            theme.edit_width
        } else {
            theme.dialog_width
        });
        match state {
            State::New(name) => {
                ui.heading("New preset");
                let mut edit = name.clone();
                let resp =
                    ui.add(egui::TextEdit::singleline(&mut edit).desired_width(f32::INFINITY));
                resp.request_focus();
                if edit != *name {
                    out.send(Msg::Name(edit.clone()));
                }
                let valid = !edit.trim().is_empty();
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid, button("Save")).clicked() || (enter && valid) {
                        let name = edit.trim().to_owned();
                        out.send(Msg::Do(Emission::SaveAs(name)));
                    }
                    if ui.add(button("Cancel")).clicked() {
                        out.send(Msg::Close);
                    }
                });
            }
            State::Edit(draft) => edit_dialog(draft, p, theme, ui, out),
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

fn edit_dialog(draft: &Draft, p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    ui.heading("Edit preset");
    let mut d = draft.clone();
    // Labels above full-width fields, so long names and descriptions fit.
    let field = |ui: &mut Ui, label: &str| {
        ui.add_space(theme.field_gap);
        ui.label(RichText::new(label).small());
    };
    field(ui, "Name");
    ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(f32::INFINITY));
    field(ui, "Description");
    ui.add(
        egui::TextEdit::multiline(&mut d.description)
            .desired_width(f32::INFINITY)
            .desired_rows(4),
    );
    field(ui, "Tags");
    ui.add(
        egui::TextEdit::singleline(&mut d.tags)
            .hint_text("comma separated")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(theme.field_gap);
    if d != *draft {
        out.send(Msg::Open(State::Edit(d.clone())));
    }
    ui.separator();
    if p.changes.is_empty() {
        ui.label(RichText::new("No unsaved changes").small());
    } else {
        ui.label(
            RichText::new(format!(
                "Unsaved changes (use Save to keep them): {}",
                p.changes.len()
            ))
            .small()
            .color(theme.dirty),
        );
        egui::ScrollArea::vertical()
            .max_height(theme.changes_height)
            .show(ui, |ui| {
                for c in p.changes {
                    ui.label(RichText::new(c).small().monospace());
                }
            });
    }
    ui.separator();
    let valid = !d.name.trim().is_empty();
    ui.horizontal(|ui| {
        if ui.add_enabled(valid, button("Save")).clicked() {
            out.send(Msg::Do(Emission::Edit {
                name: d.name.trim().to_owned(),
                description: d.description.trim().to_owned(),
                tags: parse_tags(&d.tags),
            }));
        }
        if ui.add(button("Cancel")).clicked() {
            out.send(Msg::Close);
        }
    });
}

/// `"jazz, slow ,, jazz"` → `["jazz", "slow"]`.
fn parse_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for t in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if !tags.iter().any(|x| x == t) {
            tags.push(t.to_owned());
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(state: &mut State, msg: Msg) -> Vec<Emission> {
        let p = Props {
            name: "Default".into(),
            current: None,
            presets: &[],
            roots: &[],
            folders: &[],
            folder: &(0, PathBuf::new()),
            description: "",
            tags: &[],
            changes: &[],
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

    #[test]
    fn edit_keeps_its_draft_and_tags_are_cleaned() {
        let mut s = State::default();
        let draft = Draft {
            name: "slow".into(),
            description: "Half time".into(),
            tags: "jazz, slow ,, jazz".into(),
        };
        send(&mut s, Msg::Open(State::Edit(draft.clone())));
        assert_eq!(s, State::Edit(draft));
        assert_eq!(parse_tags("jazz, slow ,, jazz"), ["jazz", "slow"]);
    }
}
