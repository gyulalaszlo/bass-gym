//! [`Rig`]: runs UI code in a real `egui::Context`, fed with input events, for tests.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Ui, pos2, vec2};

/// A headless egui context for interaction tests. Every call runs whole frames of the
/// given UI function, so components see input exactly as they would in the app.
pub struct Rig {
    pub ctx: egui::Context,
    /// Seconds; advances by one 60 Hz frame per [`Rig::frame`].
    pub time: f64,
    size: egui::Vec2,
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    /// An 800 × 500 screen.
    pub fn new() -> Self {
        Self::with_size(800.0, 500.0)
    }

    pub fn with_size(w: f32, h: f32) -> Self {
        Self {
            ctx: egui::Context::default(),
            time: 0.0,
            size: vec2(w, h),
        }
    }

    /// Runs one frame with `events` as its input.
    pub fn frame(&mut self, events: Vec<Event>, ui: &mut dyn FnMut(&mut Ui)) {
        self.time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let output = self.ctx.run_ui(input, |root| ui(root));
        output.drop_without_applying_deltas();
    }

    /// Lets `seconds` pass without input (one frame at the end).
    pub fn wait(&mut self, seconds: f64, ui: &mut dyn FnMut(&mut Ui)) {
        self.time += seconds;
        self.frame(Vec::new(), ui);
    }

    pub fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    /// Moves to `pos`, presses and releases: three frames.
    pub fn click(&mut self, pos: Pos2, ui: &mut dyn FnMut(&mut Ui)) {
        self.frame(vec![Event::PointerMoved(pos)], ui);
        self.frame(vec![Self::button(pos, true)], ui);
        self.frame(vec![Self::button(pos, false)], ui);
    }

    pub fn double_click(&mut self, pos: Pos2, ui: &mut dyn FnMut(&mut Ui)) {
        self.click(pos, ui);
        self.click(pos, ui);
    }

    /// Presses at `from`, moves to `to` in ten steps and releases.
    pub fn drag(&mut self, from: Pos2, to: Pos2, ui: &mut dyn FnMut(&mut Ui)) {
        self.frame(vec![Event::PointerMoved(from)], ui);
        self.frame(vec![Self::button(from, true)], ui);
        for i in 1..=10 {
            let at = from + (to - from) * (i as f32 / 10.0);
            self.frame(vec![Event::PointerMoved(at)], ui);
        }
        self.frame(vec![Self::button(to, false)], ui);
    }

    pub fn type_text(&mut self, text: &str, ui: &mut dyn FnMut(&mut Ui)) {
        self.frame(vec![Event::Text(text.to_owned())], ui);
    }
}
