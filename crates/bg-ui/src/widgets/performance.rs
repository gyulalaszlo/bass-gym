//! The large live performance display above the beat grid: easy to read at a glance
//! while looking at the instrument. It is a strip of [`Indicator`]s chosen by the
//! exercise (e.g. timing now, chord-tone compatibility later), side by side.

use bg_core::RecentTiming;
use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, vec2};

use crate::theme::{self, mono};

/// Black or white, whichever reads better on `fill`. Leans towards white: black text
/// is only used on light fills (pale greens / ambers), where white would wash out.
fn text_on(fill: Color32) -> Color32 {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let l = 0.2126 * lin(fill.r()) + 0.7152 * lin(fill.g()) + 0.0722 * lin(fill.b());
    if l > 0.4 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// One live indicator in the [`performance_strip`].
pub trait Indicator {
    /// Paints the indicator into `rect`.
    fn show(&self, ui: &mut Ui, rect: Rect);
}

/// Shows `indicators` side by side across the full width, `height` tall.
pub fn performance_strip(ui: &mut Ui, height: f32, indicators: &[&dyn Indicator]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if indicators.is_empty() || !ui.is_rect_visible(rect) {
        return;
    }
    let gap = 16.0;
    let n = indicators.len() as f32;
    let w = (rect.width() - gap * (n - 1.0)) / n;
    for (i, ind) in indicators.iter().enumerate() {
        let left = rect.left() + i as f32 * (w + gap);
        ind.show(ui, Rect::from_x_y_ranges(left..=left + w, rect.y_range()));
    }
}

/// Timing of the last note on a fixed ms scale (early left, late right), with the
/// spread of the recent notes as a bar behind it. The whole meter takes the colour of
/// the last note's timing grade.
pub struct TimingMeter {
    pub recent: RecentTiming,
    /// Half-width of the scale in ms (each side of 0).
    pub scale_ms: f32,
    pub good_ms: f32,
    pub ok_ms: f32,
}

impl Indicator for TimingMeter {
    fn show(&self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter_at(rect);
        let last = self.recent.last;
        let color = last.map_or(theme::dim(), |l| theme::grade_color(l.grade));

        // Pill on top, scale labels below; all within `rect`.
        let margin = 4.0;
        let label_h = 16.0;
        let pill_h = (rect.height() - 2.0 * margin - label_h).clamp(16.0, 40.0);
        let (x0, x1) = (rect.left() + margin, rect.right() - margin);
        let cx = (x0 + x1) / 2.0;
        let half = (x1 - x0) / 2.0;
        let y = rect.top() + margin + pill_h / 2.0;
        let scale = self.scale_ms.max(1.0);
        let x_of = |ms: f32| cx + (ms / scale).clamp(-1.0, 1.0) * half;

        // Axis with end ticks, tolerance ticks and the 0 mark.
        p.hline(x0..=x1, y, Stroke::new(3.0, color));
        for x in [x0, x1] {
            p.vline(x, (y - 9.0)..=(y + 9.0), Stroke::new(2.0, color));
        }
        for ms in [self.good_ms, self.ok_ms] {
            for x in [x_of(-ms), x_of(ms)] {
                p.vline(x, (y - 5.0)..=(y + 5.0), Stroke::new(1.0, theme::dim()));
            }
        }
        p.vline(
            cx,
            (y - pill_h / 2.0 - 2.0)..=(y + pill_h / 2.0 + 2.0),
            Stroke::new(3.0, color),
        );
        let label_y = y + pill_h / 2.0 + 2.0;
        p.text(
            Pos2::new(cx, label_y),
            Align2::CENTER_TOP,
            "0",
            mono(12.0),
            color,
        );
        p.text(
            Pos2::new(x0, label_y),
            Align2::LEFT_TOP,
            format!("EARLY {:.0}", -scale),
            mono(11.0),
            theme::dim(),
        );
        p.text(
            Pos2::new(x1, label_y),
            Align2::RIGHT_TOP,
            format!("+{scale:.0} LATE"),
            mono(11.0),
            theme::dim(),
        );

        // Recent spread.
        if let Some((lo, hi)) = self.recent.range {
            let h = pill_h * 0.4;
            let r = Rect::from_x_y_ranges(
                x_of(lo) - h / 2.0..=x_of(hi) + h / 2.0,
                (y - h / 2.0)..=(y + h / 2.0),
            );
            p.rect_filled(r, h / 2.0, color.gamma_multiply(0.7));
        }

        // The last note.
        let Some(last) = last else { return };
        let (text, x, beyond) = match last.timing_ms {
            Some(ms) => (format!("{ms:+.0}ms"), x_of(ms), ms.abs() > scale),
            None => ("MISS".to_owned(), cx, false),
        };
        let ink = text_on(color);
        let galley = p.layout_no_wrap(text, mono(pill_h * 0.5), ink);
        let w = (galley.size().x + pill_h * 0.5).max(pill_h);
        // Room for the off-scale arrow at either end.
        let x = x.clamp(x0 + w / 2.0 + 12.0, x1 - w / 2.0 - 12.0);
        let pill = Rect::from_center_size(Pos2::new(x, y), vec2(w, pill_h));
        p.rect_filled(pill, pill_h / 2.0, color);
        p.galley(pill.center() - galley.size() / 2.0, galley, ink);
        if beyond {
            // Off the scale: an arrow pointing outwards.
            let dir = if x > cx { 1.0 } else { -1.0 };
            let tip = Pos2::new(
                if dir > 0.0 { pill.right() } else { pill.left() } + dir * 10.0,
                y,
            );
            p.add(Shape::convex_polygon(
                vec![
                    tip,
                    tip + vec2(-dir * 8.0, -7.0),
                    tip + vec2(-dir * 8.0, 7.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
    }
}
