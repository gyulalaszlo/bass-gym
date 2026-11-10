//! Piano keyboard display. A tier-1 view.

use std::convert::Infallible;

use bg_core::music::{note_name, note_octave};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use serde::{Deserialize, Serialize};

use crate::kit::{Emit, View};
use crate::theme::Tokens;

/// Common keyboard sizes: (keys, lowest MIDI note, highest MIDI note), matching real
/// instruments (88 = A0–C8, 76 = E1–G7, 73 = E1–E7, 61 = C2–C7, ...).
pub const SIZES: [(u32, u8, u8); 7] = [
    (25, 48, 72),
    (37, 48, 84),
    (49, 36, 84),
    (61, 36, 96),
    (73, 28, 100),
    (76, 28, 103),
    (88, 21, 108),
];

pub fn range(keys: u32) -> (u8, u8) {
    SIZES
        .iter()
        .find(|s| s.0 == keys)
        .map_or((36, 96), |s| (s.1, s.2))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyLabels {
    None,
    /// Label every C with its octave.
    #[default]
    Octaves,
    /// Name every white key.
    All,
}

impl KeyLabels {
    pub const ALL: [KeyLabels; 3] = [Self::None, Self::Octaves, Self::All];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No labels",
            Self::Octaves => "C octaves",
            Self::All => "All white keys",
        }
    }
}

fn is_black(midi: u8) -> bool {
    matches!(midi % 12, 1 | 3 | 6 | 8 | 10)
}

pub struct Keyboard;

pub struct Props<'a> {
    pub keys: u32,
    /// Sounding notes.
    pub active: &'a [u8],
    /// Notes of the upcoming chord, shown as outlines.
    pub ghost: &'a [u8],
    /// The bassist's current / last note and its colour. Notes below the keyboard are
    /// shown (dashed) on the same pitch class in the lowest octave.
    pub bass: Option<(i32, Color32)>,
    pub labels: KeyLabels,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub outline: Stroke,
    /// Sounding keys, and the background black keys are filled with.
    pub active: Color32,
    pub background: Color32,
    /// Key labels, and on a sounding key.
    pub label: Color32,
    pub label_on: Color32,
    /// Label size: a share of the key width, within limits, scaled with the text.
    pub label_size: f32,
    pub label_limits: (f32, f32),
    pub text_scale: f32,
    pub ghost: Stroke,
    pub rounding: f32,
    /// Key height as a multiple of the white key width, within limits.
    pub height_ratio: f32,
    pub height_limits: (f32, f32),
    /// Black keys' width and height, as shares of the white keys'.
    pub black_width: f32,
    pub black_height: f32,
    /// Room under the keys for the bass note marker, and the marker's radius.
    pub bass_lane: f32,
    pub bass_radius: f32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            outline: Stroke::new(1.0, t.line()),
            active: t.cyan(),
            background: t.bg(),
            label: t.dim(),
            label_on: t.bg(),
            label_size: 0.45,
            label_limits: (8.0, 13.0),
            text_scale: t.text_scale,
            ghost: Stroke::new(2.5, t.magenta()),
            rounding: 2.0,
            height_ratio: 5.5,
            height_limits: (50.0, 100.0),
            black_width: 0.6,
            black_height: 0.62,
            bass_lane: 14.0,
            bass_radius: 5.0,
        }
    }
}

impl View for Keyboard {
    type Props<'a> = Props<'a>;
    type Theme = Theme;
    type Emission = Infallible;

    fn view(k: &Props, theme: &Theme, ui: &mut Ui, _out: &mut Emit<Infallible>) {
        let t = theme;
        let (low, high) = range(k.keys);
        let whites: Vec<u8> = (low..=high).filter(|&m| !is_black(m)).collect();
        let ww = ui.available_width() / whites.len() as f32;
        let (lo, hi) = t.height_limits;
        let height = (ww * t.height_ratio).clamp(lo, hi);
        let size = vec2(ui.available_width(), height + t.bass_lane);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let p = ui.painter_at(rect);
        let keys_rect = Rect::from_min_size(rect.min, vec2(rect.width(), height));

        let outline = t.outline;
        let active = t.active;
        let r = t.rounding;
        let x_of_white = |i: usize| keys_rect.left() + i as f32 * ww;
        let white_index = |m: u8| whites.iter().position(|&w| w == m);
        let key_rect = |m: u8| -> Rect {
            if is_black(m) {
                let left = white_index(m - 1).map_or(keys_rect.left(), x_of_white);
                let bw = ww * t.black_width;
                Rect::from_min_size(
                    Pos2::new(left + ww - bw / 2.0, keys_rect.top()),
                    vec2(bw, height * t.black_height),
                )
            } else {
                Rect::from_min_size(
                    Pos2::new(x_of_white(white_index(m).unwrap_or(0)), keys_rect.top()),
                    vec2(ww, height),
                )
            }
        };

        // ECAM style: keys are outlines, filled only while sounding. White keys first,
        // then the black keys on top (filled with the background to hide the lines below).
        for &m in &whites {
            let key = key_rect(m).shrink2(vec2(1.0, 0.0));
            let on = k.active.contains(&m);
            if on {
                p.rect_filled(key, r, active);
            }
            p.rect_stroke(key, r, outline, StrokeKind::Inside);
            let label = match k.labels {
                KeyLabels::All => Some(note_name(m as i32).to_owned()),
                KeyLabels::Octaves if m % 12 == 0 => Some(format!("C{}", note_octave(m as i32))),
                _ => None,
            };
            if let Some(l) = label {
                let color = if on { t.label_on } else { t.label };
                let (min, max) = t.label_limits;
                let size = (ww * t.label_size).clamp(min, max) * t.text_scale;
                p.text(
                    Pos2::new(key.center().x, key.bottom() - 4.0),
                    Align2::CENTER_BOTTOM,
                    l,
                    FontId::monospace(size),
                    color,
                );
            }
        }
        for m in (low..=high).filter(|&m| is_black(m)) {
            let key = key_rect(m);
            p.rect_filled(key.expand(1.0), r, t.background);
            let fill = if k.active.contains(&m) {
                active
            } else {
                t.background
            };
            p.rect_filled(key, r, fill);
            p.rect_stroke(key, r, outline, StrokeKind::Inside);
        }
        // Upcoming chord.
        for &m in k.ghost.iter().filter(|&&m| (low..=high).contains(&m)) {
            let key = key_rect(m).shrink(if is_black(m) { 2.0 } else { 3.0 });
            p.rect_stroke(key, r, t.ghost, StrokeKind::Inside);
        }
        // Bass note marker below the keys.
        if let Some((midi, color)) = k.bass {
            let (m, folded) = if midi < low as i32 {
                let pc = midi.rem_euclid(12) as u8;
                ((low..low + 12).find(|m| m % 12 == pc).unwrap_or(low), true)
            } else {
                (midi.min(high as i32) as u8, false)
            };
            let key = key_rect(m);
            let c = Pos2::new(key.center().x, rect.bottom() - 6.0);
            if folded {
                p.circle_stroke(c, t.bass_radius, Stroke::new(2.0, color));
            } else {
                p.circle_filled(c, t.bass_radius, color);
            }
        }
    }
}
