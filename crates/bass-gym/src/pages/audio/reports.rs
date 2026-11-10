//! The saved bug reports: how many, open the folder, clear it after asking.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bg_ui::egui::{RichText, Ui};
use bg_ui::icons;
use bg_ui::kit::{Emit, View};
use bg_ui::widgets::button::button;

use crate::pages::PageTheme;

/// How often to look at what's in the reports folder.
const REFRESH: Duration = Duration::from_secs(2);

/// What the page remembers about the folder (read from the disk in `sync`).
#[derive(Default)]
pub struct Reports {
    pub summary: bg_report::ReportsSummary,
    checked: Option<Instant>,
    /// Asking whether to delete them.
    pub confirming: bool,
    pub error: Option<String>,
}

impl Reports {
    pub fn dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "bass-gym")
            .map(|d| d.data_dir().join(bg_report::REPORTS_FOLDER))
    }

    /// Looks at the folder again every few seconds.
    pub fn sync(&mut self) {
        if let Some(dir) = Self::dir()
            && self.checked.is_none_or(|t| t.elapsed() > REFRESH)
        {
            self.summary = bg_report::summary(&dir);
            self.checked = Some(Instant::now());
        }
    }

    /// Carries out what the view asked.
    pub fn handle(&mut self, e: Emission) {
        let Some(dir) = Self::dir() else {
            return;
        };
        match e {
            Emission::Confirm => self.confirming = true,
            Emission::Cancel => self.confirming = false,
            Emission::Delete => {
                self.error = bg_report::clear(&dir).err().map(|e| format!("{e:#}"));
                self.confirming = false;
                self.checked = None;
            }
            Emission::OpenFolder => {
                let _ = std::fs::create_dir_all(&dir);
                bg_ui::folder::open_folder(&dir);
            }
        }
    }
}

pub enum Emission {
    /// Asks before deleting.
    Confirm,
    Cancel,
    Delete,
    OpenFolder,
}

pub struct ReportsView;

impl View for ReportsView {
    type Props<'a> = Reports;
    type Theme = PageTheme;
    type Emission = Emission;

    fn view(r: &Reports, theme: &PageTheme, ui: &mut Ui, out: &mut Emit<Emission>) {
        if Reports::dir().is_none() {
            return;
        }
        let bg_report::ReportsSummary { count, bytes } = r.summary;
        let plural = if count == 1 { "" } else { "s" };
        ui.label(theme.note(format!(
            "{count} saved report{plural}, {:.1} MB",
            bytes as f64 / 1_000_000.0
        )));
        ui.horizontal(|ui| {
            if r.confirming {
                ui.label(
                    RichText::new(format!(
                        "Delete {count} report{plural}? This can't be undone."
                    ))
                    .color(theme.warning),
                );
                if ui.add(button("Delete").small()).clicked() {
                    out.send(Emission::Delete);
                }
                if ui.add(button("Cancel").small()).clicked() {
                    out.send(Emission::Cancel);
                }
            } else {
                if ui
                    .add(button(format!("{} Open folder", icons::FOLDER_OPEN)).small())
                    .clicked()
                {
                    out.send(Emission::OpenFolder);
                }
                let clear = ui.add_enabled(count > 0, button("Clear report folder").small());
                if clear.clicked() {
                    out.send(Emission::Confirm);
                }
            }
        });
        if let Some(e) = &r.error {
            ui.label(RichText::new(e).small().color(theme.error));
        }
    }
}
