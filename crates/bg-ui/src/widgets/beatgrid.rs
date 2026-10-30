//! Scrolling beat grid: upcoming notes approach the "now" line from the right; played
//! notes are drawn over their targets in their grade colour as they pass.

use bg_core::{BarPlan, Matcher};
use egui::{Align2, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, vec2};

use crate::theme::{self, mono};

pub struct Beatgrid<'a> {
    pub now: f64,
    pub bars: &'a [BarPlan],
    pub matcher: &'a Matcher,
    /// Grid subdivision drawn between beats.
    pub steps_per_beat: u32,
    /// Visible span in bars before / after the now line.
    pub bars_behind: f64,
    pub bars_ahead: f64,
    /// Optional chord band: (start, end, label) spans drawn along the top.
    pub chords: &'a [(f64, f64, String)],
}

impl Beatgrid<'_> {
    pub fn show(&self, ui: &mut egui::Ui, height: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, theme::bg());
        p.rect_stroke(rect, 0.0, Stroke::new(1.0, theme::grid()), StrokeKind::Inside);

        let bar_len = self.bars.iter().rev().find(|b| b.start <= self.now).or(self.bars.first()).map_or(2.0, |b| b.duration);
        let behind = self.bars_behind * bar_len;
        let ahead = self.bars_ahead * bar_len;
        let x_now = rect.left() + rect.width() * (behind / (behind + ahead)) as f32;
        let pps = rect.width() as f64 / (behind + ahead);
        let x_of = |t: f64| x_now + ((t - self.now) * pps) as f32;
        let (t0, t1) = (self.now - behind, self.now + ahead);

        let band = if self.chords.is_empty() { 0.0 } else { 24.0 };
        let lane_top = rect.top() + 30.0 + band;
        let lane_bottom = rect.bottom() - 18.0;
        let mid = (lane_top + lane_bottom) / 2.0;

        // Grid.
        for bar in self.bars.iter().filter(|b| b.end() >= t0 && b.start <= t1) {
            let bx = x_of(bar.start);
            let bar_rect = Rect::from_x_y_ranges(bx..=x_of(bar.end()), rect.y_range());
            if bar.is_count_in() {
                p.rect_filled(bar_rect.shrink2(vec2(0.0, 1.0)), 0.0, theme::raised());
                p.text(Pos2::new(bx + 8.0, rect.top() + 6.0), Align2::LEFT_TOP, "COUNT-IN", mono(13.0), theme::amber());
            } else {
                let n = p.text(Pos2::new(bx + 8.0, rect.top() + 6.0), Align2::LEFT_TOP, format!("{}", bar.index + 1), mono(13.0), theme::text());
                let v = p.text(n.right_top() + vec2(12.0, 0.0), Align2::LEFT_TOP, format!("{:.0}", bar.bpm), mono(13.0), theme::green());
                p.text(v.right_top() + vec2(4.0, 0.0), Align2::LEFT_TOP, "BPM", mono(13.0), theme::cyan());
            }
            if bar.muted {
                let band = Rect::from_x_y_ranges(bar_rect.x_range(), (rect.bottom() - 18.0)..=(rect.bottom() - 1.0));
                p.rect_filled(band, 0.0, theme::amber().gamma_multiply(0.12));
                p.text(Pos2::new(bx + 8.0, band.center().y), Align2::LEFT_CENTER, format!("{} CLICK OFF", crate::icons::SPEAKER_SLASH), theme::prop(12.0), theme::amber());
            }
            p.vline(bx, (rect.top() + 2.0)..=(rect.bottom() - 2.0), Stroke::new(2.0, theme::line()));
            let beat = bar.duration / bar.beats.max(1) as f64;
            let steps = self.steps_per_beat.max(1);
            for b in 0..bar.beats {
                for s in 0..steps {
                    if b == 0 && s == 0 {
                        continue;
                    }
                    let x = x_of(bar.start + beat * (b as f64 + s as f64 / steps as f64));
                    let (stroke, dash) = if s == 0 { (Stroke::new(1.5, theme::dim()), 6.0) } else { (Stroke::new(1.0, theme::grid()), 3.0) };
                    p.extend(Shape::dashed_line(&[Pos2::new(x, lane_top - 6.0), Pos2::new(x, lane_bottom + 6.0)], stroke, dash, dash));
                }
            }
        }

        // Chord band.
        let band_rect = Rect::from_x_y_ranges(rect.x_range(), (rect.top() + 26.0)..=(rect.top() + 26.0 + band - 4.0));
        for (start, end, label) in self.chords.iter().filter(|c| c.1 >= t0 && c.0 <= t1) {
            let r = Rect::from_x_y_ranges(x_of(*start)..=x_of(*end), band_rect.y_range());
            let active = *start <= self.now && self.now < *end;
            p.rect_stroke(r.shrink2(vec2(1.0, 0.0)), 0.0, Stroke::new(1.0, if active { theme::cyan() } else { theme::grid() }), StrokeKind::Inside);
            p.with_clip_rect(r.intersect(rect)).text(
                Pos2::new(r.left().max(rect.left()) + 6.0, r.center().y),
                Align2::LEFT_CENTER,
                label,
                mono(13.0),
                if active { theme::cyan() } else { theme::text() },
            );
        }

        // Expected notes and their outcomes.
        let m = self.matcher;
        let first = m.expected.partition_point(|e| e.time + e.len < t0);
        for (e, o) in m.expected[first..].iter().zip(&m.outcomes[first..]) {
            if e.time > t1 {
                break;
            }
            let h = if e.accent { 30.0 } else { 20.0 };
            let r = Rect::from_x_y_ranges(x_of(e.time)..=x_of(e.time + e.len).max(x_of(e.time) + 4.0), (mid - h / 2.0)..=(mid + h / 2.0));
            match o.grade {
                Some(g) => {
                    let c = theme::grade_color(g);
                    p.rect_filled(r, 1.0, c.gamma_multiply(0.18));
                    p.rect_stroke(r, 1.0, Stroke::new(1.5, c), StrokeKind::Inside);
                }
                None => {
                    let active = e.time <= self.now && self.now <= e.time + e.len;
                    p.rect_stroke(r, 1.0, Stroke::new(if active { 2.0 } else { 1.5 }, if active { theme::cyan() } else { theme::text() }), StrokeKind::Inside);
                }
            }
        }

        // Played notes, drawn as brackets: a full-height line at the onset "[", a line at
        // the detected end "]" (absent while the note still sounds), a thin line joining
        // them, and the timing error from the target start to the onset along the top.
        for pl in m.played.iter().rev() {
            let end = pl.offset.unwrap_or(self.now);
            if end < t0 {
                break;
            }
            if pl.onset > t1 {
                continue;
            }
            let target = pl.matched.map(|i| &m.expected[i]);
            let color = match pl.matched.and_then(|i| m.outcomes[i].grade) {
                Some(g) => theme::grade_color(g),
                None => theme::red(),
            };
            // Matched notes sit on their target; extra notes in the lane above.
            let (top, bottom) = if target.is_some() { (mid - 24.0, mid + 28.0) } else { (lane_top - 6.0, lane_top + 14.0) };
            let (xs, xe) = (x_of(pl.onset), x_of(end).max(x_of(pl.onset) + 1.0));
            let tick = 6.0;
            p.line_segment([Pos2::new(xs, bottom - 2.0), Pos2::new(xe, bottom - 2.0)], Stroke::new(2.0, color.gamma_multiply(0.8)));
            p.vline(xs, top..=bottom, Stroke::new(2.5, color));
            p.hline(xs..=xs + tick, top, Stroke::new(2.5, color));
            p.hline(xs..=xs + tick, bottom, Stroke::new(2.5, color));
            if pl.offset.is_some() {
                p.vline(xe, top..=bottom, Stroke::new(2.0, color));
                p.hline(xe - tick..=xe, top, Stroke::new(2.0, color));
                p.hline(xe - tick..=xe, bottom, Stroke::new(2.0, color));
            }
            // Separate grading: harmony as a dot above the note.
            if let (bg_core::GradeMode::Separate, Some(h)) = (m.grade_mode, pl.harmony) {
                p.circle_filled(Pos2::new(xs + tick, top - 12.0), 4.0, theme::grade_color(bg_core::scoring::harmony_grade(h)));
            }
            if let Some(e) = target {
                let xt = x_of(e.time);
                if (xt - xs).abs() >= 1.0 {
                    let y = top - 5.0;
                    p.hline(xt.min(xs)..=xt.max(xs), y, Stroke::new(2.0, color));
                    p.vline(xt, (y - 3.0)..=(y + 3.0), Stroke::new(1.5, color));
                }
            }
        }

        // The now line.
        p.vline(x_now, rect.y_range(), Stroke::new(2.5, theme::magenta()));
        let t = Pos2::new(x_now, rect.top());
        p.add(Shape::convex_polygon(vec![t + vec2(0.0, 8.0), t + vec2(-6.0, 0.0), t + vec2(6.0, 0.0)], theme::magenta(), Stroke::NONE));
    }
}
