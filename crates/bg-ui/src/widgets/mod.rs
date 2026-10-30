pub mod beatgrid;
pub mod groove_editor;
pub mod histogram;
pub mod keyboard;
pub mod results;
pub mod session_panel;
pub mod tuner;

/// A big start / stop button. Returns true when clicked.
pub fn transport_button(ui: &mut egui::Ui, running: bool) -> bool {
    use crate::{icons, theme};
    let (text, color) = if running { (format!("{}  STOP", icons::STOP), theme::amber()) } else { (format!("{}  START", icons::PLAY), theme::green()) };
    ui.add_sized([120.0, 36.0], egui::Button::new(egui::RichText::new(text).strong().size(16.0).color(color)).stroke(egui::Stroke::new(1.0, color)))
        .clicked()
}
