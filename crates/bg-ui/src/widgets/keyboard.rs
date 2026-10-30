//! Piano keyboard display.

use bg_core::music::{note_name, note_octave};
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};
use serde::{Deserialize, Serialize};

use crate::theme::{self, mono};

/// Common keyboard sizes: (keys, lowest MIDI note, highest MIDI note), matching real
/// instruments (88 = A0–C8, 76 = E1–G7, 73 = E1–E7, 61 = C2–C7, ...).
pub const SIZES: [(u32, u8, u8); 7] = [(25, 48, 72), (37, 48, 84), (49, 36, 84), (61, 36, 96), (73, 28, 100), (76, 28, 103), (88, 21, 108)];

pub fn range(keys: u32) -> (u8, u8) {
    SIZES.iter().find(|s| s.0 == keys).map_or((36, 96), |s| (s.1, s.2))
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

pub struct Keyboard<'a> {
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

impl Keyboard<'_> {
    pub fn show(&self, ui: &mut egui::Ui) {
        let (low, high) = range(self.keys);
        let whites: Vec<u8> = (low..=high).filter(|&m| !is_black(m)).collect();
        let ww = ui.available_width() / whites.len() as f32;
        let height = (ww * 5.5).clamp(70.0, 190.0);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height + 14.0), Sense::hover());
        let p = ui.painter_at(rect);
        let keys_rect = Rect::from_min_size(rect.min, vec2(rect.width(), height));

        let outline = Stroke::new(1.0, theme::line());
        let active = theme::cyan();
        let x_of_white = |i: usize| keys_rect.left() + i as f32 * ww;
        let white_index = |m: u8| whites.iter().position(|&w| w == m);
        let key_rect = |m: u8| -> Rect {
            if is_black(m) {
                let left = white_index(m - 1).map_or(keys_rect.left(), x_of_white);
                let bw = ww * 0.6;
                Rect::from_min_size(Pos2::new(left + ww - bw / 2.0, keys_rect.top()), vec2(bw, height * 0.62))
            } else {
                Rect::from_min_size(Pos2::new(x_of_white(white_index(m).unwrap_or(0)), keys_rect.top()), vec2(ww, height))
            }
        };

        // ECAM style: keys are outlines, filled only while sounding. White keys first,
        // then the black keys on top (filled with the background to hide the lines below).
        for &m in &whites {
            let r = key_rect(m).shrink2(vec2(1.0, 0.0));
            let on = self.active.contains(&m);
            if on {
                p.rect_filled(r, 2.0, active);
            }
            p.rect_stroke(r, 2.0, outline, StrokeKind::Inside);
            let label = match self.labels {
                KeyLabels::All => Some(note_name(m as i32).to_owned()),
                KeyLabels::Octaves if m % 12 == 0 => Some(format!("C{}", note_octave(m as i32))),
                _ => None,
            };
            if let Some(l) = label {
                let color = if on { theme::bg() } else { theme::dim() };
                p.text(Pos2::new(r.center().x, r.bottom() - 4.0), Align2::CENTER_BOTTOM, l, mono((ww * 0.45).clamp(8.0, 13.0)), color);
            }
        }
        for m in (low..=high).filter(|&m| is_black(m)) {
            let r = key_rect(m);
            p.rect_filled(r.expand(1.0), 2.0, theme::bg());
            p.rect_filled(r, 2.0, if self.active.contains(&m) { active } else { theme::bg() });
            p.rect_stroke(r, 2.0, outline, StrokeKind::Inside);
        }
        // Upcoming chord.
        for &m in self.ghost.iter().filter(|&&m| (low..=high).contains(&m)) {
            let r = key_rect(m).shrink(if is_black(m) { 2.0 } else { 3.0 });
            p.rect_stroke(r, 2.0, Stroke::new(2.5, theme::magenta()), StrokeKind::Inside);
        }
        // Bass note marker below the keys.
        if let Some((midi, color)) = self.bass {
            let (m, folded) = if midi < low as i32 {
                let pc = midi.rem_euclid(12) as u8;
                ((low..low + 12).find(|m| m % 12 == pc).unwrap_or(low), true)
            } else {
                (midi.min(high as i32) as u8, false)
            };
            let r = key_rect(m);
            let c = Pos2::new(r.center().x, rect.bottom() - 6.0);
            if folded {
                p.circle_stroke(c, 5.0, Stroke::new(2.0, color));
            } else {
                p.circle_filled(c, 5.0, color);
            }
        }
    }
}
