//! Live / final session statistics: stat tiles and accuracy histograms.

use bg_core::scoring::{mean, std_dev};
use bg_core::{BarStats, Grade, ScoringThresholds, SessionStats};
use egui::{Align2, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, vec2};

use crate::theme::{self, mono};
use crate::widgets::button::{button, selectable_value};
use crate::widgets::histogram::Histogram;

/// The pass in which a dialog was last shown, so global shortcuts like Space (start /
/// stop) can stay out of its way.
fn modal_flag() -> egui::Id {
    egui::Id::new("bg_modal_shown_pass")
}

/// Whether a dialog drawn with [`results_dialog`] was shown on the previous (or this)
/// frame. Goes false by itself when the dialog stops being drawn (e.g. the exercise
/// was switched).
pub fn modal_open(ctx: &egui::Context) -> bool {
    let shown: Option<u64> = ctx.data(|d| d.get_temp(modal_flag()));
    shown.is_some_and(|p| p + 1 >= ctx.cumulative_pass_nr())
}

/// A tab of the results dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResultsTab {
    #[default]
    Stats,
    Review,
}

/// The results of a finished session as a modal dialog. `add` draws the contents of the
/// open tab (e.g. [`results`] and exercise-specific tables). Passing `tab` shows the tabs,
/// when there's a recording to review. Closes on Close, Space, Esc or a click outside.
pub fn results_dialog(
    ctx: &egui::Context,
    open: &mut bool,
    title: &str,
    mut tab: Option<&mut ResultsTab>,
    add: impl FnOnce(&mut egui::Ui, ResultsTab),
) {
    if !*open {
        return;
    }
    // Esc leaves a text field or a popup, and a click outside dismisses a popup. Neither
    // should also close the dialog (and leave a report half written).
    let busy = ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx);
    let reviewing = tab.as_deref() == Some(&ResultsTab::Review);
    let screen = ctx.content_rect();
    let margin = 20;
    let max_width = if reviewing { 1400.0 } else { 960.0 };
    let width = (screen.width() - 64.0 - 2.0 * margin as f32).clamp(320.0, max_width);
    let max_h = (screen.height() - 96.0).max(200.0);
    let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(margin);
    let resp = egui::Modal::new(egui::Id::new("results_dialog"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(width);
            theme::title_label(ui, title);
            ui.add_space(6.0);
            if let Some(tab) = tab.as_deref_mut() {
                ui.horizontal(|ui| {
                    selectable_value(ui, tab, ResultsTab::Stats, "Statistics");
                    selectable_value(ui, tab, ResultsTab::Review, "Score + Audio");
                });
                ui.add_space(8.0);
            }
            let shown = tab.as_deref().copied().unwrap_or_default();
            egui::ScrollArea::vertical()
                .id_salt(shown)
                .max_height(max_h - 110.0)
                .auto_shrink([false, true])
                .show(ui, |ui| add(ui, shown));
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(button("Close").min_size([96.0, 30.0])).clicked()
            })
            .inner
        });
    // Space closes too (and is used up), so a second Space starts the next session.
    let space = !ctx.egui_wants_keyboard_input()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space));
    let close = resp.inner || space || (!busy && resp.should_close());
    if close {
        *open = false;
    } else {
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| d.insert_temp(modal_flag(), pass));
    }
}

/// An ECAM-style readout: white label, coloured value, cyan unit. Read-only values are
/// not boxed; boxes are reserved for inputs.
fn tile(ui: &mut egui::Ui, label: &str, value: String, color: Color32) {
    let (value, unit) = match value.rsplit_once(' ') {
        Some((v, u)) => (v.to_owned(), u.to_owned()),
        None if value.ends_with('%') => (value.trim_end_matches('%').to_owned(), "%".to_owned()),
        None => (value, String::new()),
    };
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
        // Labels never wrap (the group frames size to their contents).
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.label(
            RichText::new(label.to_uppercase())
                .font(mono(13.0))
                .color(theme::text()),
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new(value).font(mono(22.0)).color(color));
            if !unit.is_empty() {
                ui.label(
                    RichText::new(unit.to_uppercase())
                        .font(mono(13.0))
                        .color(theme::cyan()),
                );
            }
        });
    });
}

/// A titled group of readouts in a thin frame.
fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme::grid()))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = vec2(18.0, 4.0);
                theme::caption(ui, title);
                ui.horizontal_top(add);
            });
        });
}

/// A captioned block (e.g. a table), for the `tables` row of [`results`].
pub fn block(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 4.0);
        theme::caption(ui, title);
        add(ui);
    });
}

/// Lays out blocks left to right, top-aligned, wrapping onto new rows.
fn wrap_row(ui: &mut egui::Ui, gap: egui::Vec2, add: impl FnOnce(&mut egui::Ui)) {
    ui.with_layout(
        egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
        |ui| {
            ui.spacing_mut().item_spacing = gap;
            add(ui);
        },
    );
}

/// `m:ss` (or `h:mm:ss`).
pub fn format_duration(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn rate_color(rate: Option<f32>) -> Color32 {
    match rate {
        Some(r) if r >= 0.9 => theme::green(),
        Some(r) if r >= 0.7 => theme::amber(),
        Some(_) => theme::red(),
        None => theme::dim(),
    }
}

/// Session results: summary groups, accuracy over time, histograms, then tables.
/// `tables` adds exercise-specific tables (e.g. per chord) next to the built-in ones.
pub fn results(
    ui: &mut egui::Ui,
    stats: &SessionStats,
    th: &ScoringThresholds,
    tables: impl FnOnce(&mut egui::Ui),
) {
    let fmt = |v: Option<f32>, f: &dyn Fn(f32) -> String| v.map_or("--".to_owned(), f);
    let hit = stats.hit_rate();
    let offset = mean(&stats.timing_ms);
    let spread = std_dev(&stats.timing_ms);
    let length = mean(&stats.length_ratios);
    let level_sd = std_dev(&stats.levels_db);
    let accent = mean(&stats.accent_levels_db)
        .zip(mean(&stats.levels_db))
        .map(|(a, n)| a - n);

    wrap_row(ui, vec2(12.0, 12.0), |ui| {
        group(ui, "Session", |ui| {
            tile(ui, "Time", format_duration(stats.duration_s), theme::text());
            tile(ui, "Bars", stats.bars.to_string(), theme::text());
            let bpm = stats.per_bar.iter().map(|b| b.bpm);
            let (lo, hi) = bpm.fold((f32::MAX, f32::MIN), |(lo, hi), b| (lo.min(b), hi.max(b)));
            if lo <= hi {
                let tempo = if hi - lo < 0.5 {
                    format!("{lo:.0} bpm")
                } else {
                    format!("{lo:.0}-{hi:.0} bpm")
                };
                tile(ui, "Tempo", tempo, theme::text());
            }
        });
        group(ui, "Timing", |ui| {
            tile(
                ui,
                "Hit rate",
                fmt(hit, &|r| format!("{:.0}%", r * 100.0)),
                rate_color(hit),
            );
            tile(
                ui,
                "Mean offset",
                fmt(offset, &|o| format!("{o:+.1} ms")),
                offset.map_or(theme::dim(), |o| th_color(th, o)),
            );
            tile(
                ui,
                "Spread σ",
                fmt(spread, &|s| format!("{s:.1} ms")),
                spread.map_or(theme::dim(), |s| th_color(th, s)),
            );
        });
        group(ui, "Notes", |ui| {
            tile(
                ui,
                "Length",
                fmt(length, &|l| format!("{:.0}%", l * 100.0)),
                length.map_or(theme::dim(), |l| theme::grade_color(th.length_grade(l))),
            );
        });
        group(ui, "Dynamics", |ui| {
            tile(
                ui,
                "Level σ",
                fmt(level_sd, &|s| format!("{s:.1} dB")),
                theme::text(),
            );
            tile(
                ui,
                "Accent Δ",
                fmt(accent, &|a| format!("{a:+.1} dB")),
                theme::text(),
            );
        });
        if stats.harmony.iter().sum::<usize>() > 0 {
            group(ui, "Harmony", |ui| {
                let rate = stats.harmony_rate();
                tile(
                    ui,
                    "In chord",
                    fmt(rate, &|r| format!("{:.0}%", r * 100.0)),
                    rate_color(rate),
                );
                tile(ui, "Passing", stats.harmony[1].to_string(), theme::amber());
                let clashes = stats.harmony[2];
                tile(
                    ui,
                    "Clashes",
                    clashes.to_string(),
                    if clashes > 0 {
                        theme::red()
                    } else {
                        theme::green()
                    },
                );
                tile(
                    ui,
                    "Pitch unknown",
                    stats.harmony[3].to_string(),
                    theme::dim(),
                );
            });
        }
    });

    if !stats.per_bar.is_empty() {
        ui.add_space(12.0);
        accuracy_timeline(ui, &stats.per_bar, 150.0);
    }

    ui.add_space(12.0);
    let lim = (th.ok_ms * 2.0).max(40.0).round();
    let lengths: Vec<f32> = stats.length_ratios.iter().map(|r| r * 100.0).collect();
    let levels: Vec<f32> = stats
        .levels_db
        .iter()
        .chain(&stats.accent_levels_db)
        .copied()
        .collect();
    let height = 130.0 * mono(1.0).size;
    theme::responsive_columns(ui, 240.0, 3, |ui, i| {
        // keep some margin around the histograms
        egui::Frame::default()
            .inner_margin(ui.spacing().item_spacing.x * 2.0)
            .show(ui, |ui| {
                // get the size after we're inside the frame to have proper margins
                let size = vec2(ui.available_width(), height);
                match i {
                    0 => Histogram {
                        title: "Timing (late +)",
                        values: &stats.timing_ms,
                        min: -lim,
                        max: lim,
                        bins: 24,
                        unit: "ms",
                        color: &|v| th_color(th, v),
                        target: Some(0.0),
                    }
                    .show(ui, size),
                    1 => Histogram {
                        title: "Note length",
                        values: &lengths,
                        min: 0.0,
                        max: 200.0,
                        bins: 20,
                        unit: "%",
                        color: &|v| theme::grade_color(th.length_grade(v / 100.0)),
                        target: Some(100.0),
                    }
                    .show(ui, size),
                    _ => Histogram {
                        title: "Level",
                        values: &levels,
                        min: -48.0,
                        max: 0.0,
                        bins: 24,
                        unit: "dB",
                        color: &|_| theme::cyan(),
                        target: None,
                    }
                    .show(ui, size),
                }
            });
    });

    ui.add_space(12.0);
    wrap_row(ui, vec2(40.0, 16.0), |ui| {
        block(ui, "Notes", |ui| grade_table(ui, stats));
        if stats.per_tempo.len() > 1 {
            block(ui, "Per tempo", |ui| tempo_table(ui, stats));
        }
        tables(ui);
    });
}

/// Table header cell.
fn head(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::dim()));
}

fn cell(ui: &mut egui::Ui, text: String, color: Color32) {
    ui.label(RichText::new(text).font(mono(14.0)).color(color));
}

/// Note counts per grade.
fn grade_table(ui: &mut egui::Ui, stats: &SessionStats) {
    let total = stats.total().max(1) as f32;
    egui::Grid::new("per_grade")
        .spacing(vec2(18.0, 2.0))
        .show(ui, |ui| {
            for h in ["GRADE", "NOTES", "SHARE"] {
                head(ui, h);
            }
            ui.end_row();
            for (g, label) in [
                (Grade::Good, "Good"),
                (Grade::Ok, "Ok"),
                (Grade::Poor, "Poor"),
                (Grade::Missed, "Missed"),
            ] {
                let c = theme::grade_color(g);
                let n = stats.count(g);
                cell(ui, label.to_uppercase(), c);
                cell(ui, n.to_string(), theme::text());
                cell(ui, format!("{:.0}%", n as f32 * 100.0 / total), c);
                ui.end_row();
            }
            cell(ui, "EXTRA".into(), theme::dim());
            cell(ui, stats.extra.to_string(), theme::text());
            ui.end_row();
        });
}

fn tempo_table(ui: &mut egui::Ui, stats: &SessionStats) {
    let fmt = |v: Option<f32>, f: &dyn Fn(f32) -> String| v.map_or("--".to_owned(), f);
    egui::Grid::new("per_tempo")
        .spacing(vec2(18.0, 2.0))
        .show(ui, |ui| {
            for h in ["BPM", "NOTES", "HIT", "OFFSET", "σ"] {
                head(ui, h);
            }
            ui.end_row();
            for t in &stats.per_tempo {
                let rate = t.hits as f32 / t.notes.max(1) as f32;
                let text = theme::text();
                cell(ui, format!("{:.0}", t.bpm), text);
                cell(ui, t.notes.to_string(), text);
                cell(ui, format!("{:.0}%", rate * 100.0), rate_color(Some(rate)));
                cell(ui, fmt(mean(&t.timing_ms), &|o| format!("{o:+.1}")), text);
                cell(ui, fmt(std_dev(&t.timing_ms), &|s| format!("{s:.1}")), text);
                ui.end_row();
            }
        });
}

/// Bars averaged by the trend line of [`accuracy_timeline`].
const TREND_BARS: usize = 4;

/// Hit rate per bar over the session (bar width = bar duration), with a trend line and
/// markers where the tempo changes. Hovering a bar shows its numbers.
fn accuracy_timeline(ui: &mut egui::Ui, bars: &[BarStats], height: f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter_at(rect);
    let t = p.text(
        rect.left_top(),
        Align2::LEFT_TOP,
        "ACCURACY OVER TIME",
        mono(13.0),
        theme::text(),
    );
    p.hline(
        t.x_range(),
        t.bottom() + 1.0,
        Stroke::new(1.0, theme::text()),
    );
    let axis_font = mono(11.0);
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + 36.0, t.bottom() + 22.0),
        Pos2::new(rect.right() - 4.0, rect.bottom() - axis_font.size - 6.0),
    );
    let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
        return;
    };
    let total = (last.start + last.duration - first.start).max(1e-6);
    let x_of = |t: f64| plot.left() + ((t - first.start) / total) as f32 * plot.width();
    let y_of = |r: f32| plot.bottom() - r * plot.height();

    // Hit-rate guides.
    for (r, c) in [
        (1.0, theme::grid()),
        (0.9, theme::green()),
        (0.7, theme::amber()),
        (0.5, theme::grid()),
    ] {
        p.hline(
            plot.x_range(),
            y_of(r),
            Stroke::new(1.0, c.gamma_multiply(0.4)),
        );
        // 100% should just be a line, no label (too close to 90%)
        if r < 1.0 {
            p.text(
                Pos2::new(plot.left() - 4.0, y_of(r)),
                Align2::RIGHT_CENTER,
                format!("{:.0}%", r * 100.0),
                axis_font.clone(),
                theme::dim(),
            );
        }
    }
    p.hline(
        plot.x_range(),
        plot.bottom(),
        Stroke::new(1.0, theme::line()),
    );

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
                theme::raised(),
            );
        }
        if b.notes == 0 {
            continue;
        }
        // Stacked from the bottom: good, then ok (together the hit rate), then poor,
        // fainter, as it is not a hit. Missed notes leave the rest empty.
        let mut y = plot.bottom();
        for (g, alpha) in [(Grade::Good, 0.8), (Grade::Ok, 0.8), (Grade::Poor, 0.35)] {
            // for (g, alpha) in [(Grade::Good, 0.8), (Grade::Ok, 0.8)] {
            let n = b.counts[g as usize];
            if n == 0 {
                continue;
            }
            let h = n as f32 / b.notes as f32 * plot.height();
            p.rect_filled(
                Rect::from_x_y_ranges(x0..=x1, (y - h)..=y),
                0.0,
                theme::grade_color(g).gamma_multiply(alpha),
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
        p.add(Shape::line(points, Stroke::new(2.0, theme::text())));
    }

    // Tempo changes: a marker line and the new tempo above the plot.
    let mut label_end = f32::MIN;
    let mut prev_bpm = None;
    for b in bars {
        if prev_bpm.is_some_and(|p: f32| (p - b.bpm).abs() < 0.05) {
            continue;
        }
        let x = x_of(b.start);
        let c = theme::cyan();
        if prev_bpm.is_some() {
            p.vline(x, plot.y_range(), Stroke::new(1.0, c.gamma_multiply(0.6)));
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
        p.vline(
            x,
            plot.bottom()..=plot.bottom() + 3.0,
            Stroke::new(1.0, theme::line()),
        );
        p.text(
            Pos2::new(x, plot.bottom() + 4.0),
            if i == 0 {
                Align2::LEFT_TOP
            } else {
                Align2::CENTER_TOP
            },
            (b.index + 1).to_string(),
            axis_font.clone(),
            theme::dim(),
        );
    }
    p.text(
        Pos2::new(plot.left() - 4.0, plot.bottom() + 4.0),
        Align2::RIGHT_TOP,
        "BAR",
        axis_font,
        theme::dim(),
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
                .font(mono(13.0)),
            );
        });
    }
}

fn th_color(th: &ScoringThresholds, ms: f32) -> Color32 {
    theme::grade_color(th.timing_grade(ms))
}

#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn durations() {
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(61.6), "1:02");
        assert_eq!(format_duration(3725.0), "1:02:05");
    }
}
