//! The Statistics tab: summary tiles, accuracy over time, histograms and tables. A
//! tier-1 view.

use std::convert::Infallible;

use bg_core::scoring::{mean, std_dev};
use bg_core::{Grade, ScoringThresholds, SessionStats};
use egui::{Color32, RichText, Ui, vec2};

use super::format_duration;
use super::theme::Theme;
use super::timeline::accuracy_timeline;
use crate::kit::{Emit, View, show};
use crate::theme::{caption, responsive_columns};
use crate::views::histogram::{self, Histogram};

pub struct Stats;

pub struct Props<'a> {
    pub stats: &'a SessionStats,
    pub thresholds: &'a ScoringThresholds,
    /// Exercise-specific tables, drawn next to the built-in ones.
    pub tables: Option<&'a dyn Fn(&mut Ui)>,
}

impl View for Stats {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(p: &Props, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        results(ui, theme, p.stats, p.thresholds, p.tables);
    }
}

/// An ECAM-style readout: white label, coloured value, cyan unit. Read-only values are
/// not boxed; boxes are reserved for inputs.
fn tile(ui: &mut egui::Ui, theme: &Theme, label: &str, value: String, color: Color32) {
    let t = &theme.tile;
    let (value, unit) = match value.rsplit_once(' ') {
        Some((v, u)) => (v.to_owned(), u.to_owned()),
        None if value.ends_with('%') => (value.trim_end_matches('%').to_owned(), "%".to_owned()),
        None => (value, String::new()),
    };
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = t.spacing;
        // Labels never wrap (the group frames size to their contents).
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.label(
            RichText::new(label.to_uppercase())
                .font(t.label_font.clone())
                .color(t.label),
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new(value).font(t.value_font.clone()).color(color));
            if !unit.is_empty() {
                ui.label(
                    RichText::new(unit.to_uppercase())
                        .font(t.unit_font.clone())
                        .color(t.unit),
                );
            }
        });
    });
}

/// A titled group of readouts in a thin frame.
fn group(ui: &mut egui::Ui, theme: &Theme, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let [x, y] = theme.group_margin;
    egui::Frame::new()
        .stroke(theme.group_frame)
        .inner_margin(egui::Margin::symmetric(x, y))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = theme.group_spacing;
                caption(ui, title);
                ui.horizontal_top(add);
            });
        });
}

/// A captioned block (e.g. a table), for the `tables` row of [`results`].
pub fn block(ui: &mut egui::Ui, theme: &Theme, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = theme.block_spacing;
        caption(ui, title);
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

/// Session results: summary groups, accuracy over time, histograms, then tables.
/// `tables` adds exercise-specific tables (e.g. per chord) next to the built-in ones.
fn results(
    ui: &mut egui::Ui,
    theme: &Theme,
    stats: &SessionStats,
    th: &ScoringThresholds,
    tables: Option<&dyn Fn(&mut egui::Ui)>,
) {
    let text = theme.tile.value;
    let none = theme.rates.none;
    let th_color = |ms: f32| theme.grades.get(th.timing_grade(ms));
    let fmt = |v: Option<f32>, f: &dyn Fn(f32) -> String| v.map_or("--".to_owned(), f);
    let hit = stats.hit_rate();
    let offset = mean(&stats.timing_ms);
    let spread = std_dev(&stats.timing_ms);
    let length = mean(&stats.length_ratios);
    let level_sd = std_dev(&stats.levels_db);
    let accent = mean(&stats.accent_levels_db)
        .zip(mean(&stats.levels_db))
        .map(|(a, n)| a - n);

    wrap_row(ui, theme.groups_gap, |ui| {
        group(ui, theme, "Session", |ui| {
            tile(ui, theme, "Time", format_duration(stats.duration_s), text);
            tile(ui, theme, "Bars", stats.bars.to_string(), text);
            let bpm = stats.per_bar.iter().map(|b| b.bpm);
            let (lo, hi) = bpm.fold((f32::MAX, f32::MIN), |(lo, hi), b| (lo.min(b), hi.max(b)));
            if lo <= hi {
                let tempo = if hi - lo < 0.5 {
                    format!("{lo:.0} bpm")
                } else {
                    format!("{lo:.0}-{hi:.0} bpm")
                };
                tile(ui, theme, "Tempo", tempo, text);
            }
        });
        group(ui, theme, "Timing", |ui| {
            tile(
                ui,
                theme,
                "Hit rate",
                fmt(hit, &|r| format!("{:.0}%", r * 100.0)),
                theme.rates.get(hit),
            );
            tile(
                ui,
                theme,
                "Mean offset",
                fmt(offset, &|o| format!("{o:+.1} ms")),
                offset.map_or(none, th_color),
            );
            tile(
                ui,
                theme,
                "Spread σ",
                fmt(spread, &|s| format!("{s:.1} ms")),
                spread.map_or(none, th_color),
            );
        });
        group(ui, theme, "Notes", |ui| {
            tile(
                ui,
                theme,
                "Length",
                fmt(length, &|l| format!("{:.0}%", l * 100.0)),
                length.map_or(none, |l| theme.grades.get(th.length_grade(l))),
            );
        });
        group(ui, theme, "Dynamics", |ui| {
            tile(
                ui,
                theme,
                "Level σ",
                fmt(level_sd, &|s| format!("{s:.1} dB")),
                text,
            );
            tile(
                ui,
                theme,
                "Accent Δ",
                fmt(accent, &|a| format!("{a:+.1} dB")),
                text,
            );
        });
        if stats.harmony.iter().sum::<usize>() > 0 {
            group(ui, theme, "Harmony", |ui| {
                let rate = stats.harmony_rate();
                tile(
                    ui,
                    theme,
                    "In chord",
                    fmt(rate, &|r| format!("{:.0}%", r * 100.0)),
                    theme.rates.get(rate),
                );
                tile(
                    ui,
                    theme,
                    "Passing",
                    stats.harmony[1].to_string(),
                    theme.rates.ok,
                );
                let clashes = stats.harmony[2];
                tile(
                    ui,
                    theme,
                    "Clashes",
                    clashes.to_string(),
                    if clashes > 0 {
                        theme.rates.bad
                    } else {
                        theme.rates.good
                    },
                );
                tile(
                    ui,
                    theme,
                    "Pitch unknown",
                    stats.harmony[3].to_string(),
                    none,
                );
            });
        }
    });

    if !stats.per_bar.is_empty() {
        ui.add_space(theme.section_gap);
        accuracy_timeline(ui, theme, &stats.per_bar);
    }

    ui.add_space(theme.section_gap);
    let lim = (th.ok_ms * 2.0).max(40.0).round();
    let lengths: Vec<f32> = stats.length_ratios.iter().map(|r| r * 100.0).collect();
    let levels: Vec<f32> = stats
        .levels_db
        .iter()
        .chain(&stats.accent_levels_db)
        .copied()
        .collect();
    let length_color = |v: f32| theme.grades.get(th.length_grade(v / 100.0));
    let level_color = |_: f32| theme.level;
    let height = theme.histogram_height;
    responsive_columns(ui, theme.histogram_column, 3, |ui, i| {
        // keep some margin around the histograms
        egui::Frame::default()
            .inner_margin(ui.spacing().item_spacing.x * 2.0)
            .show(ui, |ui| {
                // get the size after we're inside the frame to have proper margins
                let size = vec2(ui.available_width(), height);
                let props = match i {
                    0 => histogram::Props {
                        title: "Timing (late +)",
                        values: &stats.timing_ms,
                        min: -lim,
                        max: lim,
                        bins: 24,
                        unit: "ms",
                        color: &th_color,
                        target: Some(0.0),
                        size,
                    },
                    1 => histogram::Props {
                        title: "Note length",
                        values: &lengths,
                        min: 0.0,
                        max: 200.0,
                        bins: 20,
                        unit: "%",
                        color: &length_color,
                        target: Some(100.0),
                        size,
                    },
                    _ => histogram::Props {
                        title: "Level",
                        values: &levels,
                        min: -48.0,
                        max: 0.0,
                        bins: 24,
                        unit: "dB",
                        color: &level_color,
                        target: None,
                        size,
                    },
                };
                show::<Histogram>(ui, ("histogram", i), &props, &theme.histogram);
            });
    });

    ui.add_space(theme.section_gap);
    wrap_row(ui, theme.tables_gap, |ui| {
        block(ui, theme, "Notes", |ui| grade_table(ui, theme, stats));
        if stats.per_tempo.len() > 1 {
            block(ui, theme, "Per tempo", |ui| tempo_table(ui, theme, stats));
        }
        if let Some(tables) = tables {
            tables(ui);
        }
    });
}

/// Table header cell.
fn head(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(RichText::new(text).small().color(theme.table.head));
}

fn cell(ui: &mut egui::Ui, theme: &Theme, text: String, color: Color32) {
    ui.label(
        RichText::new(text)
            .font(theme.table.font.clone())
            .color(color),
    );
}

/// Note counts per grade.
fn grade_table(ui: &mut egui::Ui, theme: &Theme, stats: &SessionStats) {
    let text = theme.table.text;
    let total = stats.total().max(1) as f32;
    egui::Grid::new("per_grade")
        .spacing(theme.table.spacing)
        .show(ui, |ui| {
            for h in ["GRADE", "NOTES", "SHARE"] {
                head(ui, theme, h);
            }
            ui.end_row();
            for (g, label) in [
                (Grade::Good, "Good"),
                (Grade::Ok, "Ok"),
                (Grade::Poor, "Poor"),
                (Grade::Missed, "Missed"),
            ] {
                let c = theme.grades.get(g);
                let n = stats.count(g);
                cell(ui, theme, label.to_uppercase(), c);
                cell(ui, theme, n.to_string(), text);
                cell(ui, theme, format!("{:.0}%", n as f32 * 100.0 / total), c);
                ui.end_row();
            }
            cell(ui, theme, "EXTRA".into(), theme.table.extra);
            cell(ui, theme, stats.extra.to_string(), text);
            ui.end_row();
        });
}

fn tempo_table(ui: &mut egui::Ui, theme: &Theme, stats: &SessionStats) {
    let text = theme.table.text;
    let fmt = |v: Option<f32>, f: &dyn Fn(f32) -> String| v.map_or("--".to_owned(), f);
    egui::Grid::new("per_tempo")
        .spacing(theme.table.spacing)
        .show(ui, |ui| {
            for h in ["BPM", "NOTES", "HIT", "OFFSET", "σ"] {
                head(ui, theme, h);
            }
            ui.end_row();
            for t in &stats.per_tempo {
                let rate = t.hits as f32 / t.notes.max(1) as f32;
                cell(ui, theme, format!("{:.0}", t.bpm), text);
                cell(ui, theme, t.notes.to_string(), text);
                cell(
                    ui,
                    theme,
                    format!("{:.0}%", rate * 100.0),
                    theme.rates.get(Some(rate)),
                );
                cell(
                    ui,
                    theme,
                    fmt(mean(&t.timing_ms), &|o| format!("{o:+.1}")),
                    text,
                );
                cell(
                    ui,
                    theme,
                    fmt(std_dev(&t.timing_ms), &|s| format!("{s:.1}")),
                    text,
                );
                ui.end_row();
            }
        });
}
