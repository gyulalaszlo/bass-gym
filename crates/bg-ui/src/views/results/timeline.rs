//! Accuracy over time: hit rate per bar, with a trend line and tempo changes.

use bg_core::{BarStats, Grade};
use egui::{Align2, Pos2, Rect, RichText, Sense, Shape, Stroke, vec2};

use super::format_duration;
use super::theme::Theme;

/// Bars averaged by the trend line of [`accuracy_timeline`].
const TREND_BARS: usize = 4;

/// Hit rate per bar over the session (bar width = bar duration), with a trend line and
/// markers where the tempo changes. Hovering a bar shows its numbers.
pub fn accuracy_timeline(ui: &mut egui::Ui, theme: &Theme, bars: &[BarStats]) {
    let tt = &theme.timeline;
    let size = vec2(ui.available_width(), tt.height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    let t = p.text(
        rect.left_top(),
        Align2::LEFT_TOP,
        "ACCURACY OVER TIME",
        tt.title_font.clone(),
        tt.title,
    );
    p.hline(t.x_range(), t.bottom() + 1.0, Stroke::new(1.0, tt.title));
    let axis_font = tt.axis_font.clone();
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + tt.left, t.bottom() + tt.top),
        Pos2::new(rect.right() - 4.0, rect.bottom() - axis_font.size - 6.0),
    );
    let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
        return;
    };
    let total = (last.start + last.duration - first.start).max(1e-6);
    let x_of = |t: f64| plot.left() + ((t - first.start) / total) as f32 * plot.width();
    let y_of = |r: f32| plot.bottom() - r * plot.height();

    // Hit-rate guides.
    for (r, c) in [1.0, 0.9, 0.7, 0.5].into_iter().zip(tt.guides) {
        p.hline(
            plot.x_range(),
            y_of(r),
            Stroke::new(1.0, c.gamma_multiply(tt.guide_alpha)),
        );
        // No label at 100%: it would sit too close to the one at 90%.
        if r < 1.0 {
            p.text(
                Pos2::new(plot.left() - 4.0, y_of(r)),
                Align2::RIGHT_CENTER,
                format!("{:.0}%", r * 100.0),
                axis_font.clone(),
                tt.label,
            );
        }
    }
    p.hline(plot.x_range(), plot.bottom(), tt.axis);

    // Bars, coloured by hit rate.
    let hovered = resp
        .hover_pos()
        .filter(|pos| plot.x_range().contains(pos.x))
        .and_then(|pos| {
            bars.iter()
                .position(|b| pos.x >= x_of(b.start) && pos.x < x_of(b.start + b.duration))
        });
    for (i, b) in bars.iter().enumerate() {
        let (x0, x1) = (x_of(b.start), x_of(b.start + b.duration));
        let (x0, x1) = if x1 - x0 > 3.0 {
            (x0 + 1.0, x1 - 1.0)
        } else {
            (x0, x1.max(x0 + 1.0))
        };
        if hovered == Some(i) {
            p.rect_filled(
                Rect::from_x_y_ranges(x0..=x1, plot.y_range()),
                0.0,
                tt.hover,
            );
        }
        if b.notes == 0 {
            continue;
        }
        // Stacked from the bottom: good, then ok (together the hit rate), then poor,
        // fainter, as it is not a hit. Missed notes leave the rest empty.
        let mut y = plot.bottom();
        for (g, alpha) in [(Grade::Good, 0.8), (Grade::Ok, 0.8), (Grade::Poor, 0.35)] {
            let n = b.counts[g as usize];
            if n == 0 {
                continue;
            }
            let h = n as f32 / b.notes as f32 * plot.height();
            p.rect_filled(
                Rect::from_x_y_ranges(x0..=x1, (y - h)..=y),
                0.0,
                theme.grades.get(g).gamma_multiply(alpha),
            );
            y -= h;
        }
    }

    // Trend: hit rate over the last few bars (weighted by notes).
    if bars.len() > 1 {
        let points: Vec<Pos2> = (0..bars.len())
            .filter_map(|i| {
                let win = &bars[i.saturating_sub(TREND_BARS - 1)..=i];
                let notes: usize = win.iter().map(|b| b.notes).sum();
                let hits: usize = win.iter().map(|b| b.hits).sum();
                let b = &bars[i];
                (notes > 0).then(|| {
                    Pos2::new(
                        x_of(b.start + b.duration / 2.0),
                        y_of(hits as f32 / notes as f32),
                    )
                })
            })
            .collect();
        p.add(Shape::line(points, tt.trend));
    }

    // Tempo changes: a marker line and the new tempo above the plot.
    let mut label_end = f32::MIN;
    let mut prev_bpm = None;
    for b in bars {
        if prev_bpm.is_some_and(|p: f32| (p - b.bpm).abs() < 0.05) {
            continue;
        }
        let x = x_of(b.start);
        let c = tt.tempo;
        if prev_bpm.is_some() {
            let faint = c.gamma_multiply(tt.tempo_line_alpha);
            p.vline(x, plot.y_range(), Stroke::new(1.0, faint));
        }
        let text = if prev_bpm.is_none() {
            format!("{:.0} BPM", b.bpm)
        } else {
            format!("{:.0}", b.bpm)
        };
        prev_bpm = Some(b.bpm);
        if x < label_end + 6.0 {
            continue;
        }
        let r = p.text(
            Pos2::new(x, plot.top() - 3.0),
            Align2::LEFT_BOTTOM,
            text,
            axis_font.clone(),
            c,
        );
        p.vline(x, r.y_range(), Stroke::new(1.0, c));
        label_end = r.right();
    }

    // Bar numbers along the bottom, as dense as fits.
    let bar_w = plot.width() / bars.len() as f32;
    let every = [1, 2, 4, 8, 16, 32, 64, 128, 256]
        .into_iter()
        .find(|&k| bar_w * k as f32 >= 32.0)
        .unwrap_or(512);
    for (i, b) in bars.iter().enumerate().step_by(every) {
        let x = x_of(b.start);
        p.vline(x, plot.bottom()..=plot.bottom() + 3.0, tt.axis);
        p.text(
            Pos2::new(x, plot.bottom() + 4.0),
            if i == 0 {
                Align2::LEFT_TOP
            } else {
                Align2::CENTER_TOP
            },
            (b.index + 1).to_string(),
            axis_font.clone(),
            tt.label,
        );
    }
    p.text(
        Pos2::new(plot.left() - 4.0, plot.bottom() + 4.0),
        Align2::RIGHT_TOP,
        "BAR",
        axis_font,
        tt.label,
    );

    if let Some(b) = hovered.map(|i| bars[i]) {
        resp.on_hover_ui_at_pointer(|ui| {
            ui.label(
                RichText::new(format!(
                    "Bar {}  {:.0} bpm  {}\n{}/{} hits\ngood {}  ok {}  poor {}  missed {}",
                    b.index + 1,
                    b.bpm,
                    format_duration(b.start),
                    b.hits,
                    b.notes,
                    b.counts[Grade::Good as usize],
                    b.counts[Grade::Ok as usize],
                    b.counts[Grade::Poor as usize],
                    b.counts[Grade::Missed as usize],
                ))
                .font(tt.tooltip_font.clone()),
            );
        });
    }
}
