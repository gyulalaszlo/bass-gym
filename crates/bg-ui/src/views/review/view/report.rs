//! The report row: title, notes and export, then how the export went.

use egui::{RichText, Ui};

use super::Props;
use crate::icons;
use crate::kit::{Emit, field};
use crate::views::review::Msg;
use crate::views::review::model::Export;
use crate::views::review::theme::Theme;
use crate::widgets::button::button;
use crate::widgets::controls::text_field;

pub fn report_row(p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
    let r = p.review;
    let m = &theme.metrics;
    let running = matches!(r.export, Export::Running(_));
    ui.horizontal(|ui| {
        field(ui, &r.report_title, out, Msg::ReportTitle, |ui, v| {
            ui.add(text_field(v).width(m.title_width).hint("Report title"))
        });
        let notes = ui.available_width() - m.export_button.x - m.group_gap;
        field(ui, &r.report_notes, out, Msg::ReportNotes, |ui, v| {
            ui.add(
                text_field(v)
                    .width(notes.max(m.notes_min_width))
                    .hint("What went wrong?"),
            )
        });
        let export = ui.add_enabled(
            p.data_dir.is_some() && !running,
            button("Export selected bars").min_size(m.export_button),
        );
        if export.clicked()
            && let Some(dir) = p.data_dir
        {
            out.send(Msg::Export(dir.to_owned()));
        }
    });
    match &r.export {
        Export::Idle => {}
        Export::Running(_) => {
            // Keep polling the export thread.
            ui.ctx().request_repaint();
            ui.label(RichText::new("Exporting...").small().color(theme.hint));
        }
        Export::Failed(e) => {
            ui.label(
                RichText::new(format!("Export failed: {e}"))
                    .small()
                    .color(theme.failed),
            );
        }
        Export::Done { folder, copied } => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Saved to {} (and as .zip)", folder.display()))
                        .small()
                        .color(theme.done),
                );
                let label = if *copied {
                    format!("{} Copied", icons::CHECK)
                } else {
                    format!("{} Copy path", icons::COPY)
                };
                if ui.add(button(label).small()).clicked() {
                    // Output to the system through egui, like painting.
                    ui.ctx().copy_text(folder.display().to_string());
                    out.send(Msg::Copied);
                }
                if ui
                    .add(button(format!("{} Open folder", icons::FOLDER_OPEN)).small())
                    .clicked()
                {
                    out.send(Msg::OpenFolder);
                }
            });
        }
    }
}
