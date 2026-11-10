//! The "Per chord" table of the results: targets, hit rate and chord fit per
//! progression step. A tier-1 view.

use std::convert::Infallible;

use bg_ui::egui::{self, Color32, FontId, RichText, Ui, Vec2, vec2};
use bg_ui::kit::{Emit, View};
use bg_ui::theme::Tokens;

pub struct PerChordTable;

#[derive(Clone, Debug, PartialEq)]
pub struct PerChord {
    /// The preset has rhythm targets, so NOTES and HIT are shown.
    pub targets: bool,
    /// Chord fit is graded, so IN CHORD is shown.
    pub fit: bool,
    pub rows: Vec<Row>,
}

/// One progression step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub label: String,
    /// Decided targets, and those hit.
    pub notes: usize,
    pub hits: usize,
    /// Notes judged for harmony, and those that fit.
    pub judged: usize,
    pub in_chord: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub font: FontId,
    pub head: Color32,
    pub chord: Color32,
    pub spacing: Vec2,
}

impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self {
            font: t.mono(14.0),
            head: t.dim(),
            chord: t.cyan(),
            spacing: vec2(18.0, 2.0),
        }
    }
}

/// `a` of `b` as a whole percentage, or `--` with nothing to count.
pub fn percent(a: usize, b: usize) -> String {
    if b == 0 {
        "--".to_owned()
    } else {
        format!("{:.0}%", a as f32 * 100.0 / b as f32)
    }
}

impl View for PerChordTable {
    type Props<'a> = PerChord;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(p: &PerChord, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        let cell = |ui: &mut Ui, text: String| {
            ui.label(RichText::new(text).font(theme.font.clone()));
        };
        egui::Grid::new("per_chord")
            .spacing(theme.spacing)
            .show(ui, |ui| {
                let heads = [
                    ("CHORD", true),
                    ("NOTES", p.targets),
                    ("HIT", p.targets),
                    ("IN CHORD", p.fit),
                ];
                for (h, _) in heads.iter().filter(|h| h.1) {
                    ui.label(RichText::new(*h).small().color(theme.head));
                }
                ui.end_row();
                for r in &p.rows {
                    ui.label(
                        RichText::new(&r.label)
                            .font(theme.font.clone())
                            .color(theme.chord),
                    );
                    if p.targets {
                        cell(ui, r.notes.to_string());
                        cell(ui, percent(r.hits, r.notes));
                    }
                    if p.fit {
                        cell(ui, percent(r.in_chord, r.judged));
                    }
                    ui.end_row();
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_rounds_and_shows_nothing_to_count() {
        assert_eq!(percent(1, 3), "33%");
        assert_eq!(percent(0, 0), "--");
    }
}
