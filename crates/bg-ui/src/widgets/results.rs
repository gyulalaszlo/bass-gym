//! Live / final session statistics: stat tiles and accuracy histograms.

use bg_core::scoring::{mean, std_dev};
use bg_core::{Grade, ScoringThresholds, SessionStats};
use egui::{Color32, RichText, vec2};

use crate::theme::{self, mono};
use crate::widgets::histogram::Histogram;

/// An ECAM-style readout: white label, coloured value, cyan unit. Read-only values are
/// not boxed; boxes are reserved for inputs.
fn tile(ui: &mut egui::Ui, label: &str, value: String, color: Color32) {
    let (value, unit) = match value.rsplit_once(' ') {
        Some((v, u)) => (v.to_owned(), u.to_owned()),
        None if value.ends_with('%') => (value.trim_end_matches('%').to_owned(), "%".to_owned()),
        None => (value, String::new()),
    };
    ui.vertical(|ui| {
        ui.label(RichText::new(label.to_uppercase()).font(mono(13.0)).color(theme::text()));
        ui.horizontal(|ui| {
            ui.label(RichText::new(value).font(mono(22.0)).color(color));
            if !unit.is_empty() {
                ui.label(RichText::new(unit.to_uppercase()).font(mono(13.0)).color(theme::cyan()));
            }
        });
    });
    ui.add_space(10.0);
}

pub fn results(ui: &mut egui::Ui, stats: &SessionStats, th: &ScoringThresholds) {
    let fmt = |v: Option<f32>, f: &dyn Fn(f32) -> String| v.map_or("--".to_owned(), f);
    let hit = stats.hit_rate();
    let hit_color = match hit {
        Some(r) if r >= 0.9 => theme::green(),
        Some(r) if r >= 0.7 => theme::amber(),
        Some(_) => theme::red(),
        None => theme::dim(),
    };
    let offset = mean(&stats.timing_ms);
    let spread = std_dev(&stats.timing_ms);
    let length = mean(&stats.length_ratios);
    let level_sd = std_dev(&stats.levels_db);
    let accent = mean(&stats.accent_levels_db).zip(mean(&stats.levels_db)).map(|(a, n)| a - n);

    ui.horizontal_wrapped(|ui| {
        tile(ui, "Hit rate", fmt(hit, &|r| format!("{:.0}%", r * 100.0)), hit_color);
        let off_color = offset.map_or(theme::dim(), |o| th_color(th, o));
        tile(ui, "Mean offset", fmt(offset, &|o| format!("{o:+.1} ms")), off_color);
        tile(ui, "Spread σ", fmt(spread, &|s| format!("{s:.1} ms")), spread.map_or(theme::dim(), |s| th_color(th, s)));
        tile(ui, "Length", fmt(length, &|l| format!("{:.0}%", l * 100.0)), length.map_or(theme::dim(), |l| theme::grade_color(th.length_grade(l))));
        tile(ui, "Level σ", fmt(level_sd, &|s| format!("{s:.1} dB")), theme::text());
        tile(ui, "Accent Δ", fmt(accent, &|a| format!("{a:+.1} dB")), theme::text());
        ui.vertical(|ui| {
            for (g, label) in [(Grade::Good, "GOOD"), (Grade::Ok, "OK"), (Grade::Poor, "POOR"), (Grade::Missed, "MISSED")] {
                ui.label(RichText::new(format!("{label:<7}{:>4}", stats.count(g))).font(mono(13.0)).color(theme::grade_color(g)));
            }
            ui.label(RichText::new(format!("{:<7}{:>4}", "EXTRA", stats.extra)).font(mono(13.0)).color(theme::dim()));
        });
    });
    if stats.harmony.iter().sum::<usize>() > 0 {
        let rate = stats.harmony_rate();
        let color = match rate {
            Some(r) if r >= 0.9 => theme::green(),
            Some(r) if r >= 0.7 => theme::amber(),
            Some(_) => theme::red(),
            None => theme::dim(),
        };
        ui.horizontal_wrapped(|ui| {
            tile(ui, "In chord", fmt(rate, &|r| format!("{:.0}%", r * 100.0)), color);
            tile(ui, "Passing", stats.harmony[1].to_string(), theme::amber());
            tile(ui, "Clashes", stats.harmony[2].to_string(), if stats.harmony[2] > 0 { theme::red() } else { theme::green() });
            tile(ui, "Pitch unknown", stats.harmony[3].to_string(), theme::dim());
        });
    }
    if let Some(o) = offset {
        let text = if o.abs() <= th.good_ms / 2.0 {
            "On the beat".to_owned()
        } else if o > 0.0 {
            format!("Dragging: on average {o:.0} ms behind the beat")
        } else {
            format!("Rushing: on average {:.0} ms ahead of the beat", -o)
        };
        ui.label(RichText::new(text).color(theme::dim()));
    }

    ui.add_space(4.0);
    let lim = (th.ok_ms * 2.0).max(40.0).round();
    let lengths: Vec<f32> = stats.length_ratios.iter().map(|r| r * 100.0).collect();
    let levels: Vec<f32> = stats.levels_db.iter().chain(&stats.accent_levels_db).copied().collect();
    let height = 130.0 * mono(1.0).size;
    theme::responsive_columns(ui, 240.0, 3, |ui, i| {
        let size = vec2(ui.available_width(), height);
        match i {
            0 => Histogram { title: "Timing (late +)", values: &stats.timing_ms, min: -lim, max: lim, bins: 24, unit: "ms", color: &|v| th_color(th, v), target: Some(0.0) }
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
            _ => Histogram { title: "Level", values: &levels, min: -48.0, max: 0.0, bins: 24, unit: "dB", color: &|_| theme::cyan(), target: None }.show(ui, size),
        }
    });

    if stats.per_tempo.len() > 1 {
        ui.add_space(4.0);
        theme::caption(ui, "Per tempo");
        egui::Grid::new("per_tempo").striped(false).spacing(vec2(18.0, 2.0)).show(ui, |ui| {
            for h in ["BPM", "NOTES", "HIT", "OFFSET", "σ"] {
                ui.label(RichText::new(h).small().color(theme::dim()));
            }
            ui.end_row();
            for t in &stats.per_tempo {
                let rate = t.hits as f32 / t.notes.max(1) as f32;
                ui.label(RichText::new(format!("{:.0}", t.bpm)).font(mono(14.0)));
                ui.label(RichText::new(t.notes.to_string()).font(mono(14.0)));
                ui.label(RichText::new(format!("{:.0}%", rate * 100.0)).font(mono(14.0)).color(if rate >= 0.9 { theme::green() } else if rate >= 0.7 { theme::amber() } else { theme::red() }));
                ui.label(RichText::new(fmt(mean(&t.timing_ms), &|o| format!("{o:+.1}"))).font(mono(14.0)));
                ui.label(RichText::new(fmt(std_dev(&t.timing_ms), &|s| format!("{s:.1}"))).font(mono(14.0)));
                ui.end_row();
            }
        });
    }
}

fn th_color(th: &ScoringThresholds, ms: f32) -> Color32 {
    theme::grade_color(th.timing_grade(ms))
}
