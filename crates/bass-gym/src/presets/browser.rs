//! The preset folder browser: a dropdown holding Finder-style columns. The first column
//! lists the preset dirs, and each next one the subfolders of the folder picked before it.
//! A view: it emits the folder clicked, and the owner keeps the selection.

use std::path::PathBuf;

use bg_ui::egui::{self, Ui};
use bg_ui::icons;
use bg_ui::kit::{Emit, View};
use bg_ui::widgets::button::selectable;
use bg_ui::widgets::controls::dropdown_panel;

pub struct Browser;

pub struct Props<'a> {
    /// A display name per preset dir.
    pub roots: &'a [String],
    pub folders: &'a [(usize, PathBuf)],
    pub selected: &'a (usize, PathBuf),
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    pub target: (usize, PathBuf),
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub column_width: f32,
    pub column_height: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            column_width: 160.0,
            column_height: 280.0,
        }
    }
}

impl View for Browser {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = (usize, PathBuf);

    fn view(p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<(usize, PathBuf)>) {
        let text = format!("{}  {}", icons::FOLDER_SIMPLE, label(p.roots, p.selected));
        dropdown_panel(ui, "preset_folder", text, p.width, |ui| {
            ui.horizontal_top(|ui| {
                for (i, column) in columns(p.roots, p.folders, p.selected)
                    .into_iter()
                    .enumerate()
                {
                    if i > 0 {
                        ui.separator();
                    }
                    ui.vertical(|ui| {
                        ui.set_width(theme.column_width);
                        egui::ScrollArea::vertical()
                            .id_salt(i)
                            .max_height(theme.column_height)
                            .show(ui, |ui| {
                                for e in column {
                                    let r = ui.add(selectable(e.selected, &e.label));
                                    if r.clicked() {
                                        out.send(e.target);
                                    }
                                }
                            });
                    });
                }
            });
        });
    }
}

/// The closed browser's text: `User / slow / swing`.
fn label(roots: &[String], (root, path): &(usize, PathBuf)) -> String {
    let mut parts = vec![roots.get(*root).cloned().unwrap_or_default()];
    parts.extend(path.iter().map(|c| c.to_string_lossy().into_owned()));
    parts.join(" / ")
}

/// The browser's columns: the dirs, then the subfolders along the selected path, then the
/// selected folder's own subfolders (when it has any).
/// Entries on the selected path are marked selected, as in Finder.
pub fn columns(
    roots: &[String],
    folders: &[(usize, PathBuf)],
    selected: &(usize, PathBuf),
) -> Vec<Vec<Entry>> {
    let (root, path) = selected;
    let mut out = vec![
        roots
            .iter()
            .enumerate()
            .map(|(r, name)| Entry {
                label: name.clone(),
                target: (r, PathBuf::new()),
                selected: r == *root,
            })
            .collect::<Vec<_>>(),
    ];
    let mut parent = PathBuf::new();
    let mut along = path.iter();
    loop {
        let children: Vec<Entry> = folders
            .iter()
            .filter(|(r, f)| r == root && f.parent() == Some(parent.as_path()))
            .map(|(r, f)| Entry {
                label: f
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                target: (*r, f.clone()),
                selected: path.starts_with(f),
            })
            .collect();
        if children.is_empty() {
            break;
        }
        out.push(children);
        match along.next() {
            Some(c) => parent.push(c),
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(c: &[Entry]) -> Vec<(&str, bool)> {
        c.iter()
            .map(|e| (e.target.1.to_str().unwrap(), e.selected))
            .collect()
    }

    #[test]
    fn columns_follow_the_selected_path() {
        let folders: Vec<(usize, PathBuf)> = [(0, "a"), (0, "a/x"), (0, "a/y"), (0, "b"), (1, "c")]
            .into_iter()
            .map(|(r, p)| (r, PathBuf::from(p)))
            .collect();
        let roots = ["User".to_owned(), "Pack".to_owned()];
        let cols = columns(&roots, &folders, &(0, PathBuf::from("a")));
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0][1].label, "Pack");
        assert_eq!(
            cols[0].iter().map(|e| e.selected).collect::<Vec<_>>(),
            [true, false]
        );
        assert_eq!(names(&cols[1]), [("a", true), ("b", false)]);
        assert_eq!(names(&cols[2]), [("a/x", false), ("a/y", false)]);

        let cols = columns(&roots, &folders, &(0, PathBuf::from("a/y")));
        assert_eq!(names(&cols[2]), [("a/x", false), ("a/y", true)]);
        assert_eq!(cols.len(), 3, "a leaf adds no empty column");

        let cols = columns(&roots, &folders, &(1, PathBuf::new()));
        assert!(cols[0][1].selected);
        assert_eq!(names(&cols[1]), [("c", false)]);
    }
}
