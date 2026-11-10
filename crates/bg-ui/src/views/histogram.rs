//! A histogram of values in bins, with a title, edge labels and an optional target line.
//! A tier-1 view.

use std::convert::Infallible;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2, vec2};

use crate::kit::{Emit, View};
use crate::theme::Tokens;

pub struct Histogram;

pub struct Props<'a> {
    pub title: &'a str,
    pub values: &'a [f32],
    pub min: f32,
    pub max: f32,
    pub bins: usize,
    pub unit: &'a str,
    /// Colour of a bin given its centre value.
    pub color: &'a dyn Fn(f32) -> Color32,
    /// Optional reference value drawn as a line (e.g. 0 ms).
    pub target: Option<f32>,
    pub size: Vec2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub title_font: FontId,
    pub title: Color32,
    pub axis: Stroke,
    pub target: Stroke,
    pub label_font: FontId,
    pub label: Color32,
    /// Between the title and the plot, and the plot and its labels.
    pub title_gap: f32,
    pub label_gap: f32,
    pub bar_gap: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            title_font: t.mono(13.0),
            title: t.text(),
            axis: Stroke::new(1.0, t.line()),
            target: Stroke::new(1.0, t.magenta()),
            label_font: t.mono(12.0),
            label: t.dim(),
            title_gap: 8.0,
            label_gap: 4.0,
            bar_gap: 1.0,
        }
    }
}

impl View for Histogram {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(h: &Props, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        let (rect, _) = ui.allocate_exact_size(h.size, Sense::hover());
        let p = ui.painter_at(rect);
        let t = p.text(
            rect.left_top(),
            Align2::LEFT_TOP,
            h.title.to_uppercase(),
            theme.title_font.clone(),
            theme.title,
        );
        p.hline(t.x_range(), t.bottom() + 1.0, Stroke::new(1.0, theme.title));
        let plot = Rect::from_min_max(
            rect.min + vec2(0.0, t.height() + theme.title_gap),
            rect.max - vec2(0.0, theme.label_font.size + theme.label_gap),
        );
        p.hline(plot.x_range(), plot.bottom(), theme.axis);

        let mut counts = vec![0usize; h.bins.max(1)];
        let w = (h.max - h.min) / counts.len() as f32;
        for &v in h.values {
            let i =
                (((v - h.min) / w).floor() as isize).clamp(0, counts.len() as isize - 1) as usize;
            counts[i] += 1;
        }
        let peak = counts.iter().copied().max().unwrap_or(0).max(1) as f32;
        let bw = plot.width() / counts.len() as f32;
        for (i, &c) in counts.iter().enumerate() {
            if c == 0 {
                continue;
            }
            let bar = c as f32 / peak * plot.height();
            let gap = theme.bar_gap;
            let r = Rect::from_min_max(
                Pos2::new(plot.left() + i as f32 * bw + gap, plot.bottom() - bar),
                Pos2::new(plot.left() + (i + 1) as f32 * bw - gap, plot.bottom()),
            );
            p.rect_filled(r, 0.0, (h.color)(h.min + (i as f32 + 0.5) * w));
        }
        let x_of = |v: f32| plot.left() + (v - h.min) / (h.max - h.min) * plot.width();
        if let Some(t) = h.target {
            p.vline(x_of(t), plot.y_range(), theme.target);
        }
        // Edge labels are aligned inwards so they are not clipped.
        for (v, align) in [
            (h.min, Align2::LEFT_BOTTOM),
            ((h.min + h.max) / 2.0, Align2::CENTER_BOTTOM),
            (h.max, Align2::RIGHT_BOTTOM),
        ] {
            p.text(
                Pos2::new(x_of(v), rect.bottom()),
                align,
                format!("{v}{}", h.unit),
                theme.label_font.clone(),
                theme.label,
            );
        }
        if h.values.is_empty() {
            p.text(
                plot.center(),
                Align2::CENTER_CENTER,
                "NO DATA",
                theme.title_font.clone(),
                theme.label,
            );
        }
    }
}
