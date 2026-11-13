//! Bass neck display: a scale's notes on the fretboard, its roots, and the notes to play.
//! A tier-1 view.

use std::convert::Infallible;

use bg_core::harmony::Scale;
use egui::{Align2, Color32, FontId, Pos2, Sense, Stroke, Ui, vec2};
use serde::{Deserialize, Serialize};

use crate::kit::{Emit, View};
use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tuning {
    #[default]
    Standard4,
    DropD4,
    Standard5,
    /// Five strings with a high C instead of a low B.
    HighC5,
}

impl Tuning {
    pub const ALL: [Tuning; 4] = [Self::Standard4, Self::DropD4, Self::Standard5, Self::HighC5];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard4 => "4 · EADG",
            Self::DropD4 => "4 · DADG",
            Self::Standard5 => "5 · BEADG",
            Self::HighC5 => "5 · EADGC",
        }
    }

    /// The open strings' MIDI notes, lowest string first.
    pub fn strings(self) -> &'static [u8] {
        match self {
            Self::Standard4 => &[28, 33, 38, 43],
            Self::DropD4 => &[26, 33, 38, 43],
            Self::Standard5 => &[23, 28, 33, 38, 43],
            Self::HighC5 => &[28, 33, 38, 43, 48],
        }
    }
}

pub struct Neck;

pub struct Props<'a> {
    pub tuning: Tuning,
    pub frets: u8,
    pub scale: Scale,
    /// The note to play now, filled wherever it can be played.
    pub target: Option<u8>,
    /// The note after it, outlined.
    pub next: Option<u8>,
    /// Note names by pitch class, for the scale's notes.
    pub names: &'a [String; 12],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub string: Stroke,
    pub fret: Stroke,
    pub nut: Stroke,
    pub inlay: Color32,
    pub tone: Stroke,
    pub background: Color32,
    pub root: Color32,
    pub target: Color32,
    pub next: Stroke,
    pub label: Color32,
    pub label_on: Color32,
    pub fret_label: Color32,
    pub font: FontId,
    /// Space between strings, room left of the nut for open strings, and below for the
    /// fret numbers.
    pub string_gap: f32,
    pub open_width: f32,
    pub footer: f32,
    pub radius: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            string: Stroke::new(1.5, t.dim()),
            fret: Stroke::new(1.0, t.line()),
            nut: Stroke::new(4.0, t.dim()),
            inlay: t.grid(),
            tone: Stroke::new(1.5, t.text()),
            background: t.bg(),
            root: t.green(),
            target: t.magenta(),
            next: Stroke::new(2.5, t.magenta()),
            label: t.text(),
            label_on: t.bg(),
            fret_label: t.dim(),
            font: t.mono(10.0),
            string_gap: t.em(22.0),
            open_width: t.em(28.0),
            footer: t.em(16.0),
            radius: t.em(9.0),
        }
    }
}

impl View for Neck {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(n: &Props, t: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        let strings = n.tuning.strings();
        let height = t.string_gap * strings.len() as f32 + t.footer;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let p = ui.painter_at(rect);
        let nut_x = rect.left() + t.open_width;
        // shortcut: frets are evenly spaced, not at their real (shrinking) distances.
        let fret_w = (rect.right() - nut_x) / n.frets.max(1) as f32;
        let fret_x = |f: u8| nut_x + f as f32 * fret_w;
        // A note's spot: in the middle of its fret, or left of the nut when open.
        let note_x = |f: u8| {
            if f == 0 {
                rect.left() + t.open_width / 2.0
            } else {
                fret_x(f) - fret_w / 2.0
            }
        };
        // The lowest string at the bottom, as in tab.
        let string_y =
            |s: usize| rect.top() + t.string_gap * (strings.len() - s) as f32 - t.string_gap / 2.0;
        let (top, bottom) = (string_y(strings.len() - 1), string_y(0));
        let mid = (top + bottom) / 2.0;

        for f in [3u8, 5, 7, 9, 15, 17, 19, 21]
            .into_iter()
            .filter(|&f| f <= n.frets)
        {
            p.circle_filled(Pos2::new(note_x(f), mid), t.radius * 0.5, t.inlay);
        }
        for f in [12u8, 24].into_iter().filter(|&f| f <= n.frets) {
            let gap = t.string_gap;
            p.circle_filled(Pos2::new(note_x(f), mid - gap), t.radius * 0.5, t.inlay);
            p.circle_filled(Pos2::new(note_x(f), mid + gap), t.radius * 0.5, t.inlay);
        }
        for f in 1..=n.frets {
            p.line_segment(
                [Pos2::new(fret_x(f), top), Pos2::new(fret_x(f), bottom)],
                t.fret,
            );
            if matches!(f, 3 | 5 | 7 | 9 | 12 | 15 | 17 | 19 | 21 | 24) {
                p.text(
                    Pos2::new(note_x(f), rect.bottom()),
                    Align2::CENTER_BOTTOM,
                    f.to_string(),
                    t.font.clone(),
                    t.fret_label,
                );
            }
        }
        p.line_segment([Pos2::new(nut_x, top), Pos2::new(nut_x, bottom)], t.nut);
        for s in 0..strings.len() {
            let y = string_y(s);
            p.line_segment([Pos2::new(nut_x, y), Pos2::new(rect.right(), y)], t.string);
        }

        let root = n.scale.root % 12;
        for (s, &open) in strings.iter().enumerate() {
            for f in 0..=n.frets {
                let midi = open + f;
                let pc = midi % 12;
                if !n.scale.contains(pc) {
                    continue;
                }
                let c = Pos2::new(note_x(f), string_y(s));
                let on = n.target == Some(midi);
                let fill = if on {
                    Some(t.target)
                } else if pc == root {
                    Some(t.root)
                } else {
                    None
                };
                match fill {
                    Some(fill) => {
                        p.circle_filled(c, t.radius, fill);
                    }
                    // Filled with the background, to hide the string below.
                    None => {
                        p.circle(c, t.radius, t.background, t.tone);
                    }
                }
                if n.next == Some(midi) {
                    p.circle_stroke(c, t.radius + 3.0, t.next);
                }
                let label = if fill.is_some() { t.label_on } else { t.label };
                p.text(
                    c,
                    Align2::CENTER_CENTER,
                    &n.names[pc as usize],
                    t.font.clone(),
                    label,
                );
            }
        }
    }
}
