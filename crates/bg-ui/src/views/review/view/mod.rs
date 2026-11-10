//! The review's view: the controls, the report row, then (scrolling) the score rows or
//! the marker editor. It only reads the review and sends [`Msg`]s, which its owner
//! routes to [`super::update`].

mod controls;
mod editor;
mod report;
mod rows;

use std::path::Path;

use egui::{Key, Modifiers, Ui};

use super::Msg;
use super::model::Review;
use super::theme::Theme;
use crate::kit::{Emit, View};
use crate::widgets::score::StaffLayout;

pub struct ReviewView;

pub struct Props<'a> {
    pub review: &'a Review,
    /// Where reports go; export is off without one.
    pub data_dir: Option<&'a Path>,
    /// Distance between staff lines (the results score size preference).
    pub staff_spacing: f32,
}

impl Props<'_> {
    fn layout(&self) -> StaffLayout {
        StaffLayout {
            spacing: self.staff_spacing,
            ..self.review.layout
        }
    }
}

impl View for ReviewView {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    /// The review's own messages: its owner routes them back to it.
    type Emission = Msg;

    fn view(p: &Props, theme: &Theme, ui: &mut Ui, out: &mut Emit<Msg>) {
        let r = p.review;
        if !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space))
        {
            out.send(Msg::TogglePlay);
        }
        if r.is_playing() {
            ui.ctx().request_repaint();
        }
        let m = &theme.metrics;
        controls::controls(p, theme, ui, out);
        ui.add_space(m.controls_gap);
        report::report_row(p, theme, ui, out);
        ui.add_space(m.report_gap);
        let playhead = r.playhead();
        // Only this part scrolls: the controls above stay in view.
        let height = ui.available_height();
        egui::ScrollArea::vertical()
            .id_salt("session_review")
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if r.editor.is_some() {
                    editor::editor(p, theme, ui, playhead, out);
                } else {
                    rows::rows(p, theme, ui, playhead, out);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::{Rig, show};
    use crate::theme::Tokens;
    use crate::views::review::ReviewInput;
    use bg_audio::Recorder;
    use std::cell::RefCell;
    use std::sync::Arc;

    #[test]
    fn space_toggles_play_and_an_idle_review_sends_nothing() {
        let review = Review::new(ReviewInput {
            bars: Vec::new(),
            matcher: Default::default(),
            chords: Vec::new(),
            scale: None,
            a4: 440.0,
            recording: Arc::new(Recorder::new(1024).finish(0.0, 0.0)),
            exercise: "test".into(),
            settings: serde_json::Value::Null,
        });
        let theme = Theme::new(&Tokens::default());
        let sent = RefCell::new(Vec::new());
        let mut ui_fn = |ui: &mut Ui| {
            let props = Props {
                review: &review,
                data_dir: None,
                staff_spacing: 8.0,
            };
            sent.borrow_mut()
                .extend(show::<ReviewView>(ui, "review", &props, &theme));
        };
        let space = egui::Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let mut rig = Rig::new();
        rig.frame(Vec::new(), &mut ui_fn);
        assert!(sent.borrow().is_empty(), "an idle review sends nothing");
        rig.frame(vec![space], &mut ui_fn);
        assert!(matches!(sent.borrow().as_slice(), [Msg::TogglePlay]));
    }
}
