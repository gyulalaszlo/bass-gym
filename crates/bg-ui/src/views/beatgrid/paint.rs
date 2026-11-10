//! Drawing the beat grid. Reads only its props, theme and layout: no input, no state.

use bg_core::score::{self, ScoreInput};
use egui::{Align2, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, vec2};

use super::{Layout, LiveScore, Props, Theme};
use crate::widgets::score::{ScoreView, paint_fixed_lead};

/// A painter clipped to `rect` as well as to wherever `p` was already clipped.
fn clipped(p: &Painter, rect: Rect) -> Painter {
    p.with_clip_rect(p.clip_rect().intersect(rect))
}

pub fn paint(p: &Painter, props: &Props, theme: &Theme, l: &Layout) {
    if let (Some(live), Some(staff)) = (props.score, l.staff) {
        paint_score(&clipped(p, staff), props, theme, live, staff, l);
    }
    let p = clipped(p, l.grid);
    p.rect_filled(l.grid, 0.0, theme.bg);
    p.rect_stroke(l.grid, 0.0, theme.frame, StrokeKind::Inside);
    paint_bars(&p, props, theme, l);
    paint_chords(&p, props, theme, l);
    paint_expected(&p, props, theme, l);
    paint_played(&p, props, theme, l);
    paint_now(&p, props, theme, l);
}

/// The played notes up to now, on the same time axis as the grid.
///
/// `ScoreView` and `paint_fixed_lead` aren't migrated yet and still read the global
/// theme.
fn paint_score(p: &Painter, props: &Props, theme: &Theme, live: LiveScore, rect: Rect, l: &Layout) {
    p.rect_filled(rect, 0.0, theme.bg);
    // Whole bars in view, plus the one before for notes tied into them.
    let first = props
        .bars
        .partition_point(|b| b.end() < l.t0)
        .saturating_sub(1);
    let last = props.bars.partition_point(|b| b.start <= l.t1);
    let bars = score::build(&ScoreInput {
        bars: &props.bars[first..last.max(first)],
        matcher: props.matcher,
        chords: live.chords,
        scale: live.scale,
        a4: live.a4,
        until: props.now,
    });
    ScoreView {
        bars: &bars,
        axis: l.axis,
        layout: live.layout(),
        lead: None,
    }
    .paint(p, rect);
    let key = live.scale.map(|s| score::KeySignature::of(&s));
    paint_fixed_lead(p, rect, live.layout(), key);
    p.vline(
        l.axis.x(props.now),
        rect.y_range(),
        Stroke::new(2.5, theme.now),
    );
}

/// Per visible bar: count-in fill, bar number and BPM, "click off" band if muted, the bar
/// line, then dashed beat and step lines (the grid, not expected notes).
fn paint_bars(p: &Painter, props: &Props, theme: &Theme, l: &Layout) {
    let (rect, t, m) = (l.grid, &theme.bar, &theme.metrics);
    let x_of = |time: f64| l.axis.x(time);
    for bar in props
        .bars
        .iter()
        .filter(|b| b.end() >= l.t0 && b.start <= l.t1)
    {
        let bx = x_of(bar.start);
        let bar_rect = Rect::from_x_y_ranges(bx..=x_of(bar.end()), rect.y_range());
        let header = Pos2::new(bx, rect.top()) + m.label_inset;
        if bar.is_count_in() {
            p.rect_filled(bar_rect.shrink2(vec2(0.0, 1.0)), 0.0, t.count_in_fill);
            p.text(
                header,
                Align2::LEFT_TOP,
                "COUNT-IN",
                t.font.clone(),
                t.count_in_text,
            );
        } else {
            let n = p.text(
                header,
                Align2::LEFT_TOP,
                format!("{}", bar.index + 1),
                t.font.clone(),
                t.number,
            );
            let v = p.text(
                n.right_top() + vec2(m.label_gap, 0.0),
                Align2::LEFT_TOP,
                format!("{:.0}", bar.bpm),
                t.font.clone(),
                t.bpm,
            );
            p.text(
                v.right_top() + vec2(m.unit_gap, 0.0),
                Align2::LEFT_TOP,
                "BPM",
                t.font.clone(),
                t.unit,
            );
        }
        if bar.muted {
            let band = Rect::from_x_y_ranges(
                bar_rect.x_range(),
                (rect.bottom() - m.footer)..=(rect.bottom() - 1.0),
            );
            p.rect_filled(band, 0.0, t.muted_fill);
            p.text(
                Pos2::new(bx + m.label_inset.x, band.center().y),
                Align2::LEFT_CENTER,
                format!("{} CLICK OFF", crate::icons::SPEAKER_SLASH),
                t.muted_font.clone(),
                t.muted_text,
            );
        }
        p.vline(bx, (rect.top() + 2.0)..=(rect.bottom() - 2.0), t.line);
        let beat = bar.duration / bar.beats.max(1) as f64;
        let steps = props.steps_per_beat.max(1);
        for b in 0..bar.beats {
            for s in 0..steps {
                if b == 0 && s == 0 {
                    continue;
                }
                let x = x_of(bar.start + beat * (b as f64 + s as f64 / steps as f64));
                let (stroke, dash) = if s == 0 {
                    (t.beat_line, 6.0)
                } else {
                    (t.step_line, 3.0)
                };
                p.extend(Shape::dashed_line(
                    &[
                        Pos2::new(x, l.lane_top - 6.0),
                        Pos2::new(x, l.lane_bottom + 6.0),
                    ],
                    stroke,
                    dash,
                    dash,
                ));
            }
        }
    }
}

fn paint_chords(p: &Painter, props: &Props, theme: &Theme, l: &Layout) {
    let Some(band) = l.chord_band else {
        return;
    };
    let t = &theme.chord;
    for (start, end, label) in props.chords.iter().filter(|c| c.1 >= l.t0 && c.0 <= l.t1) {
        let r = Rect::from_x_y_ranges(l.axis.x(*start)..=l.axis.x(*end), band.y_range());
        let active = *start <= props.now && props.now < *end;
        let (frame, text) = if active {
            (t.active, t.active)
        } else {
            (t.frame, t.text)
        };
        p.rect_stroke(
            r.shrink2(vec2(1.0, 0.0)),
            0.0,
            Stroke::new(1.0, frame),
            StrokeKind::Inside,
        );
        clipped(p, r).text(
            Pos2::new(
                r.left().max(l.grid.left()) + theme.metrics.chord_inset,
                r.center().y,
            ),
            Align2::LEFT_CENTER,
            label,
            t.font.clone(),
            text,
        );
    }
}

/// Expected notes as rectangles (taller if accented): an outline until decided, then
/// filled and outlined in their grade colour.
fn paint_expected(p: &Painter, props: &Props, theme: &Theme, l: &Layout) {
    let (m, t) = (props.matcher, &theme.note);
    let x_of = |time: f64| l.axis.x(time);
    let first = m.expected.partition_point(|e| e.time + e.len < l.t0);
    for (e, o) in m.expected[first..].iter().zip(&m.outcomes[first..]) {
        if e.time > l.t1 {
            break;
        }
        let h = if e.accent {
            theme.metrics.accent_height
        } else {
            theme.metrics.note_height
        };
        let r = Rect::from_x_y_ranges(
            x_of(e.time)..=x_of(e.time + e.len).max(x_of(e.time) + 4.0),
            (l.mid - h / 2.0)..=(l.mid + h / 2.0),
        );
        match o.grade {
            Some(g) => {
                let c = t.grades.get(g);
                p.rect_filled(r, 1.0, c.gamma_multiply(t.graded_fill));
                p.rect_stroke(r, 1.0, Stroke::new(1.5, c), StrokeKind::Inside);
            }
            None => {
                let active = e.time <= props.now && props.now <= e.time + e.len;
                let stroke = if active {
                    Stroke::new(2.0, t.active)
                } else {
                    Stroke::new(1.5, t.pending)
                };
                p.rect_stroke(r, 1.0, stroke, StrokeKind::Inside);
            }
        }
    }
}

/// Played notes, drawn as brackets: a full-height line at the onset "[", a line at the
/// detected end "]" (absent while the note still sounds), a thin line joining them, and
/// the timing error from the target start to the onset along the top.
fn paint_played(p: &Painter, props: &Props, theme: &Theme, l: &Layout) {
    let (m, t) = (props.matcher, &theme.note);
    let x_of = |time: f64| l.axis.x(time);
    for pl in m.played.iter().rev() {
        let end = pl.offset.unwrap_or(props.now);
        if end < l.t0 {
            break;
        }
        if pl.onset > l.t1 {
            continue;
        }
        let target = pl.matched.and_then(|i| m.expected.get(i));
        let outcome = pl.matched.and_then(|i| m.outcomes.get(i));
        let color = outcome
            .and_then(|o| o.grade)
            .map_or(t.extra, |g| t.grades.get(g));
        // Matched notes sit on their target; extra notes in the lane above.
        let (top, bottom) = if target.is_some() {
            (l.mid - 24.0, l.mid + 28.0)
        } else {
            (l.lane_top - 6.0, l.lane_top + 14.0)
        };
        let (xs, xe) = (x_of(pl.onset), x_of(end).max(x_of(pl.onset) + 1.0));
        let tick = theme.metrics.bracket_tick;
        p.line_segment(
            [Pos2::new(xs, bottom - 2.0), Pos2::new(xe, bottom - 2.0)],
            Stroke::new(2.0, color.gamma_multiply(0.8)),
        );
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
            p.circle_filled(
                Pos2::new(xs + tick, top - 12.0),
                4.0,
                t.grades.get(bg_core::scoring::harmony_grade(h)),
            );
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
}

fn paint_now(p: &Painter, props: &Props, theme: &Theme, l: &Layout) {
    let x_now = l.axis.x(props.now);
    p.vline(x_now, l.grid.y_range(), Stroke::new(2.5, theme.now));
    let top = Pos2::new(x_now, l.grid.top());
    p.add(Shape::convex_polygon(
        vec![
            top + vec2(0.0, 8.0),
            top + vec2(-6.0, 0.0),
            top + vec2(6.0, 0.0),
        ],
        theme.now,
        Stroke::NONE,
    ));
}
