//! The preferences window: the page list, and the selected page beside it.

use bg_ui::egui::{self, Color32, Ui, Vec2, vec2};
use bg_ui::kit::{Emit, View};
use bg_ui::theme::Tokens;
use bg_ui::widgets::button::selectable;

/// The page list. A tier-1 view: emits the page clicked.
pub struct PageList;

pub struct Props<'a> {
    /// Icon and title of each page.
    pub pages: &'a [(&'static str, &'static str)],
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub fill: Color32,
    pub margin: i8,
    pub list_width: f32,
    pub item: Vec2,
    /// Smallest window, and its size the first time it opens.
    pub min_size: Vec2,
    pub size: Vec2,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            fill: t.panel(),
            margin: 14,
            list_width: 190.0,
            item: vec2(186.0, 30.0),
            min_size: vec2(560.0, 320.0),
            size: vec2(860.0, 620.0),
        }
    }
}

impl View for PageList {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = usize;

    fn view(p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<usize>) {
        for (i, (icon, title)) in p.pages.iter().enumerate() {
            let text = format!("{icon}  {title}");
            if ui
                .add(selectable(i == p.selected, text).min_size(theme.item))
                .clicked()
            {
                out.send(i);
            }
        }
    }
}

/// The two columns of the window: `list` on the left, `page` scrolling on the right.
pub fn columns(
    ui: &mut Ui,
    theme: &Theme,
    page_id: usize,
    list: impl FnOnce(&mut Ui),
    page: impl FnOnce(&mut Ui),
) {
    let avail = ui.available_size();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            vec2(theme.list_width, avail.y),
            egui::Layout::top_down(egui::Align::Min),
            list,
        );
        ui.separator();
        // The page: sections stacked vertically, scrolling when they overflow.
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), avail.y),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("prefs_page", page_id))
                    .auto_shrink([false, false])
                    .max_height(avail.y)
                    .show(ui, page);
            },
        );
    });
}
