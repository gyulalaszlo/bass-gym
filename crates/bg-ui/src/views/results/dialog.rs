//! The results dialog: a modal with a title, optional tabs, a body and a Close button.
//! A container, like `egui::Modal`: its owner draws the body.

use super::ResultsTab;
use super::theme::DialogTheme;
use crate::theme::title_label;
use crate::widgets::button::{button, selectable_value};

/// The pass in which a dialog was last shown, so global shortcuts like Space (start /
/// stop) can stay out of its way.
fn modal_flag() -> egui::Id {
    egui::Id::new("bg_modal_shown_pass")
}

/// Whether a dialog drawn with [`results_dialog`] was shown on the previous (or this)
/// frame. Goes false by itself when the dialog stops being drawn (e.g. the exercise
/// was switched).
pub fn modal_open(ctx: &egui::Context) -> bool {
    let shown: Option<u64> = ctx.data(|d| d.get_temp(modal_flag()));
    shown.is_some_and(|p| p + 1 >= ctx.cumulative_pass_nr())
}

/// The results of a finished session as a modal dialog. `add` draws the contents of the
/// open tab (e.g. [`results`] and exercise-specific tables). Passing `tab` shows the tabs,
/// when there's a recording to review. Closes on Close, Space, Esc or a click outside.
pub fn results_dialog(
    ctx: &egui::Context,
    t: &DialogTheme,
    open: &mut bool,
    title: &str,
    mut tab: Option<&mut ResultsTab>,
    add: impl FnOnce(&mut egui::Ui, ResultsTab),
) {
    if !*open {
        return;
    }
    // Esc leaves a text field or a popup, and a click outside dismisses a popup. Neither
    // should also close the dialog (and leave a report half written).
    let busy = ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx);
    let reviewing = tab.as_deref() == Some(&ResultsTab::Review);
    let screen = ctx.content_rect();
    let margin = t.margin;
    let max_width = if reviewing {
        t.review_max_width
    } else {
        t.max_width
    };
    let width =
        (screen.width() - t.screen_margin - 2.0 * margin as f32).clamp(t.min_width, max_width);
    let max_h = (screen.height() - t.height_margin).max(200.0);
    let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(margin);
    let resp = egui::Modal::new(egui::Id::new("results_dialog"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(width);
            title_label(ui, title);
            ui.add_space(t.title_gap);
            if let Some(tab) = tab.as_deref_mut() {
                ui.horizontal(|ui| {
                    selectable_value(ui, tab, ResultsTab::Stats, "Statistics");
                    selectable_value(ui, tab, ResultsTab::Review, "Score + Audio");
                });
                ui.add_space(t.tabs_gap);
            }
            let shown = tab.as_deref().copied().unwrap_or_default();
            let body_height = max_h - t.chrome_height;
            if reviewing {
                // Scrolls by itself, to keep its toolbar in view.
                ui.vertical(|ui| {
                    ui.set_max_height(body_height);
                    add(ui, shown);
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt(shown)
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| add(ui, shown));
            }
            ui.add_space(t.close_gap);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(button("Close").min_size(t.close_button)).clicked()
            })
            .inner
        });
    // Space closes too (and is used up), so a second Space starts the next session.
    let space = !ctx.egui_wants_keyboard_input()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space));
    let close = resp.inner || space || (!busy && resp.should_close());
    if close {
        *open = false;
    } else {
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| d.insert_temp(modal_flag(), pass));
    }
}
