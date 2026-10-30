//! Minimal histogram painter.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, vec2};

use crate::theme::{self, mono};

pub struct Histogram<'a> {
    pub title: &'a str,
    pub values: &'a [f32],
    pub min: f32,
    pub max: f32,
    pub bins: usize,
    pub unit: &'a str,
    /// Colour of a bin given its centre value.
    pub color: &'a dyn Fn(f32) -> Color32,
    /// Optional reference value drawn as a magenta line (e.g. 0 ms).
    pub target: Option<f32>,
}

impl Histogram<'_> {
    pub fn show(&self, ui: &mut egui::Ui, size: egui::Vec2) {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let p = ui.painter_at(rect);
        let t = p.text(rect.left_top(), Align2::LEFT_TOP, self.title.to_uppercase(), mono(13.0), theme::text());
        p.hline(t.x_range(), t.bottom() + 1.0, Stroke::new(1.0, theme::text()));
        let plot = Rect::from_min_max(rect.min + vec2(0.0, t.height() + 8.0), rect.max - vec2(0.0, mono(12.0).size + 4.0));
        p.hline(plot.x_range(), plot.bottom(), Stroke::new(1.0, theme::line()));

        let mut counts = vec![0usize; self.bins.max(1)];
        let w = (self.max - self.min) / counts.len() as f32;
        for &v in self.values {
            let i = (((v - self.min) / w).floor() as isize).clamp(0, counts.len() as isize - 1) as usize;
            counts[i] += 1;
        }
        let peak = counts.iter().copied().max().unwrap_or(0).max(1) as f32;
        let bw = plot.width() / counts.len() as f32;
        for (i, &c) in counts.iter().enumerate() {
            if c == 0 {
                continue;
            }
            let h = c as f32 / peak * plot.height();
            let r = Rect::from_min_max(Pos2::new(plot.left() + i as f32 * bw + 1.0, plot.bottom() - h), Pos2::new(plot.left() + (i + 1) as f32 * bw - 1.0, plot.bottom()));
            p.rect_filled(r, 0.0, (self.color)(self.min + (i as f32 + 0.5) * w));
        }
        let x_of = |v: f32| plot.left() + (v - self.min) / (self.max - self.min) * plot.width();
        if let Some(t) = self.target {
            p.vline(x_of(t), plot.y_range(), Stroke::new(1.0, theme::magenta()));
        }
        for v in [self.min, (self.min + self.max) / 2.0, self.max] {
            p.text(Pos2::new(x_of(v), rect.bottom()), Align2::CENTER_BOTTOM, format!("{v}{}", self.unit), mono(12.0), theme::dim());
        }
        if self.values.is_empty() {
            p.text(plot.center(), Align2::CENTER_CENTER, "NO DATA", mono(13.0), theme::dim());
        }
    }
}
