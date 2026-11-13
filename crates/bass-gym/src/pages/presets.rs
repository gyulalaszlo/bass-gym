//! The Presets page: which dirs are scanned for presets, and a Rescan button.
//!
//! The user's own dir always comes first and can't be removed. The page tells the app to
//! rescan through a shared flag, since `PrefsCtx` has no way to reach the preset store.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bg_ui::egui::{RichText, Ui};
use bg_ui::kit::{self, Emit, ThemeCache, View};
use bg_ui::widgets::button::button;
use bg_ui::{PreferencePage, PrefsCtx, icons};
use serde::{Deserialize, Serialize};

use super::{PRESETS, PageTheme, grid, help};

/// The preset dirs besides the user's own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetDirs {
    pub extra: Vec<PathBuf>,
}

impl PresetDirs {
    /// Every dir to scan, the user's own first.
    pub fn all(&self, user: &Path) -> Vec<PathBuf> {
        std::iter::once(user.to_path_buf())
            .chain(self.extra.iter().cloned())
            .collect()
    }
}

/// A dir's name in the browser: "User" for the user's own, else its last component.
pub fn root_name(i: usize, dir: &Path) -> String {
    if i == 0 {
        return "User".to_owned();
    }
    dir.file_name().map_or_else(
        || dir.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

pub struct PresetsPage {
    user: PathBuf,
    /// The path being typed.
    draft: String,
    rescan: Rc<Cell<bool>>,
    theme: ThemeCache<PageTheme>,
}

impl PresetsPage {
    pub fn new(user: PathBuf, rescan: Rc<Cell<bool>>) -> Self {
        Self {
            user,
            draft: String::new(),
            rescan,
            theme: ThemeCache::default(),
        }
    }
}

struct Dirs;

struct Props<'a> {
    dirs: &'a PresetDirs,
    user: &'a Path,
    draft: &'a str,
}

enum Emission {
    Dirs(PresetDirs),
    Draft(String),
    Rescan,
}

impl View for Dirs {
    type Props<'a> = Props<'a>;
    type Theme = PageTheme;
    type Emission = Emission;

    fn view(p: &Props, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<Emission>) {
        help(
            ui,
            theme,
            "Presets are found in these folders and all folders inside them. Copy a folder \
             of presets into one of them, or add its path here, then rescan.",
        );
        ui.add_space(theme.section_gap);
        let mut remove = None;
        // The remove button comes first, so a long path can't push it out of view.
        grid("preset_dirs", theme).show(ui, |ui| {
            ui.label(theme.note("yours"));
            path_label(ui, RichText::new(p.user.display().to_string()), p.user);
            ui.end_row();
            for (i, dir) in p.dirs.extra.iter().enumerate() {
                if ui
                    .add(button(icons::TRASH))
                    .on_hover_text("Stop scanning this folder (its files stay)")
                    .clicked()
                {
                    remove = Some(i);
                }
                let mut text = RichText::new(dir.display().to_string());
                if !dir.is_dir() {
                    text = text.color(theme.warning);
                }
                path_label(ui, text, dir);
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            let mut d = p.dirs.clone();
            d.extra.remove(i);
            out.send(Emission::Dirs(d));
        }
        ui.add_space(theme.gap);
        ui.horizontal(|ui| {
            let mut draft = p.draft.to_owned();
            let r =
                ui.add(bg_ui::egui::TextEdit::singleline(&mut draft).hint_text("/path/to/presets"));
            if draft != p.draft {
                out.send(Emission::Draft(draft.clone()));
            }
            let path = PathBuf::from(draft.trim());
            let valid = !draft.trim().is_empty() && path != p.user && !p.dirs.extra.contains(&path);
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(bg_ui::egui::Key::Enter));
            if ui
                .add_enabled(valid, button(format!("{} Add", icons::PLUS)))
                .clicked()
                || (enter && valid)
            {
                let mut d = p.dirs.clone();
                d.extra.push(path);
                out.send(Emission::Dirs(d));
                out.send(Emission::Draft(String::new()));
            }
        });
        ui.add_space(theme.section_gap);
        if ui
            .add(button(format!("{} Rescan", icons::ARROWS_CLOCKWISE)))
            .on_hover_text("Look for added, moved or removed presets")
            .clicked()
        {
            out.send(Emission::Rescan);
        }
    }
}

/// A path cut to fit, in full on hover.
fn path_label(ui: &mut Ui, text: RichText, dir: &Path) {
    let hover = if dir.is_dir() {
        dir.display().to_string()
    } else {
        format!("{} (doesn't exist)", dir.display())
    };
    ui.add(bg_ui::egui::Label::new(text).truncate())
        .on_hover_text(hover);
}

impl PreferencePage for PresetsPage {
    fn id(&self) -> &'static str {
        PRESETS
    }
    fn title(&self) -> &'static str {
        "Presets"
    }
    fn icon(&self) -> &'static str {
        icons::FOLDERS
    }

    fn ui(&mut self, ui: &mut Ui, ctx: &mut PrefsCtx) {
        let theme = self.theme.get();
        let dirs: PresetDirs = ctx.settings.get(PRESETS);
        let props = Props {
            dirs: &dirs,
            user: &self.user,
            draft: &self.draft,
        };
        for e in kit::show::<Dirs>(ui, "presets", &props, &theme) {
            match e {
                Emission::Dirs(d) => {
                    ctx.settings.set(PRESETS, &d);
                    self.rescan.set(true);
                }
                Emission::Draft(s) => self.draft = s,
                Emission::Rescan => self.rescan.set(true),
            }
        }
    }
}
