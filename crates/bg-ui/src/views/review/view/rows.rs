//! The session as rows of whole bars: the score over the waveform, on one time axis per
//! row. Click the score to select a bar and play from there, the waveform to open the
//! marker editor.

use bg_core::score::{self, KeySignature, ScoreBar};
use egui::{Align2, Painter, Rangef, Rect, RichText, Sense, StrokeKind, Ui, vec2};

use super::Props;

/// Generated from `rows.layout.xml`.
mod xml {
    include!(concat!(env!("OUT_DIR"), "/layouts/rows.rs"));
}
use crate::kit::Emit;
use crate::views::marker_editor::paint_marker;
use crate::views::review::theme::Theme;
use crate::views::review::{Msg, RecordingPeaks, click_start};
use crate::widgets::score::{Lead, ScoreView, lead_width};
use crate::widgets::time_axis::TimeAxis;
use crate::widgets::waveform::{self, WaveStyle};

pub fn rows(p: &Props, theme: &Theme, ui: &mut Ui, playhead: Option<f64>, out: &mut Emit<Msg>) {
    let r = p.review;
    let m = &theme.metrics;
    if r.score.is_empty() {
        ui.label(RichText::new("No bars were played.").color(theme.hint));
        return;
    }
    let layout = p.layout();
    let width = ui.available_width();
    let key = r.input.scale.map(|s| KeySignature::of(&s));
    let lead = lead_width(layout.spacing, true, key);
    let per_row = score::bars_per_row(&r.score, width - lead, layout.spacing);
    let score_h = layout.height();
    let row_h = score_h + m.note_lane + m.wave_height;
    let rows = score::rows(r.score.len(), per_row);
    let rows_len = rows.len();
    let mut revealed = false;
    for (n, range) in rows.into_iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
        ui.add_space(m.row_gap);
        let bars = &r.score[range];
        let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
            continue;
        };
        if let Some(t) = r.reveal
            && !revealed
            && (t < last.end || n + 1 == rows_len)
        {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
            revealed = true;
            out.send(Msg::Revealed);
        }
        if !ui.is_rect_visible(rect) {
            continue;
        }
        // A short last row keeps the scale of the full ones.
        let bar_len = (last.end - first.start) / bars.len() as f64;
        let row_end = first.start + bar_len * per_row as f64;
        let left = rect.left() + lead;
        let axis = TimeAxis::new(Rangef::new(left, rect.right()), first.start, row_end);
        let row = xml::layout(
            rect,
            &xml::ReviewRowInputs {
                bars_width: axis.x(last.end) - rect.left(),
                score_height: score_h,
                note_lane: m.note_lane,
                lead,
            },
        );
        let (staff, wave) = (row.staff, row.wave);
        let painter = ui.painter_at(rect.expand2(vec2(0.0, 2.0)));
        paint_selection(&painter, p, theme, bars, axis, rect);
        ScoreView {
            bars,
            axis,
            layout,
            lead: Some(Lead {
                clef: n == 0,
                key,
                sig: Some(first.sig),
            }),
            ghosts: &[],
        }
        .paint(&painter, staff, &theme.score);
        paint_wave(
            &painter,
            p,
            theme,
            wave,
            axis,
            (first.start, last.end),
            rect,
        );
        if let Some(t) = playhead
            && (first.start..last.end).contains(&t)
        {
            painter.vline(axis.x(t), rect.y_range(), theme.playhead);
        }
        if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked()) {
            let t = axis.t(pos.x);
            if pos.y >= wave.top() {
                out.send(Msg::OpenEditor(t));
            } else if let Some(bar) = bars.iter().find(|b| b.start <= t && t < b.end) {
                let extend = ui.input(|i| i.modifiers.shift);
                out.send(Msg::SelectBar {
                    bar: bar.index + 1,
                    extend,
                });
                if !extend {
                    let from = click_start(bar, &r.input.matcher, axis, pos.x);
                    out.send(Msg::PlayFrom(from));
                }
            }
        }
    }
}

fn paint_selection(
    painter: &Painter,
    p: &Props,
    theme: &Theme,
    bars: &[ScoreBar],
    axis: TimeAxis,
    rect: Rect,
) {
    let r = p.review;
    for b in bars {
        if (r.from_bar - 1..r.to_bar).contains(&b.index) {
            let sel = Rect::from_x_y_ranges(axis.x(b.start)..=axis.x(b.end), rect.y_range());
            painter.rect_filled(sel, 0.0, theme.selection);
        }
    }
}

fn paint_wave(
    painter: &Painter,
    p: &Props,
    theme: &Theme,
    wave: Rect,
    axis: TimeAxis,
    (from, to): (f64, f64),
    row: Rect,
) {
    let r = p.review;
    let rounding = theme.metrics.wave_rounding;
    painter.rect_filled(wave, rounding, theme.wave_bg);
    painter.rect_stroke(wave, rounding, theme.wave_frame, StrokeKind::Inside);
    let rec = &r.input.recording;
    if rec.has_audio() {
        let style = WaveStyle {
            log: r.log,
            color: theme.wave,
        };
        waveform::paint(painter, wave, axis, &RecordingPeaks(rec), style);
        paint_missing_audio(painter, p, theme, wave, axis, (from, to));
    } else {
        label(painter, theme, wave, "NO AUDIO RECORDED");
    }
    for m in &r.overlays {
        let end = m.end.unwrap_or(m.time);
        if end >= from && m.time < to && m.label.is_empty() {
            paint_marker(painter, &theme.editor.marker, axis, m, row, wave);
        }
    }
    for m in r.markers.iter().filter(|m| (from..to).contains(&m.time)) {
        paint_marker(painter, &theme.editor.marker, axis, m, row, wave);
    }
}

/// Marks audio that was overwritten or lost.
fn paint_missing_audio(
    painter: &Painter,
    p: &Props,
    theme: &Theme,
    wave: Rect,
    axis: TimeAxis,
    (from, to): (f64, f64),
) {
    let rec = &p.review.input.recording;
    let gone = rec.first_time().min(to);
    if gone > from {
        let r = Rect::from_x_y_ranges(axis.x(from)..=axis.x(gone), wave.y_range());
        painter.rect_filled(r, 0.0, theme.overwritten);
        label(painter, theme, r, "OVERWRITTEN");
    }
    for g in rec
        .gaps
        .iter()
        .filter(|g| g.time < to && g.time + g.seconds > from)
    {
        let x = axis.x(g.time);
        let r = Rect::from_x_y_ranges(x..=axis.x(g.time + g.seconds).max(x + 2.0), wave.y_range());
        painter.rect_filled(r, 0.0, theme.gap);
    }
}

fn label(painter: &Painter, theme: &Theme, rect: Rect, text: &str) {
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        theme.label_font.clone(),
        theme.label,
    );
}
