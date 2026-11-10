//! Paints a [`bg_core::score`] bass staff along a [`TimeAxis`], so notes line up with
//! whatever shares that axis. Notation glyphs come from the Bravura music font.

use bg_core::TimeSignature;
use bg_core::score::{
    Accidental, KeySignature, MIDDLE_LINE, ScoreBar, ScoreEvent, ScoreKind, ScoreNote, TOP_LINE,
};
use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, Rangef, Rect, Shape, Stroke, pos2};

use crate::theme::Tokens;
use crate::widgets::time_axis::TimeAxis;

/// Name of the bundled music font's family.
pub const MUSIC_FONT: &str = "music";

// Glyphs of the SMuFL standard.
const F_CLEF: char = '\u{E062}';
const HEAD_WHOLE: char = '\u{E0A2}';
const HEAD_HALF: char = '\u{E0A3}';
const HEAD_BLACK: char = '\u{E0A4}';
const HEAD_X: char = '\u{E0A9}';
const FLAT: char = '\u{E260}';
const NATURAL: char = '\u{E261}';
const SHARP: char = '\u{E262}';
const SIG_ZERO: u32 = 0xE080;
const FLAGS_UP: [char; 4] = ['\u{E240}', '\u{E242}', '\u{E244}', '\u{E246}'];
const FLAGS_DOWN: [char; 4] = ['\u{E241}', '\u{E243}', '\u{E245}', '\u{E247}'];
const REST_WHOLE: char = '\u{E4E3}';
const REST_HALF: char = '\u{E4E4}';
const REST_QUARTER: char = '\u{E4E5}';
const RESTS_FLAGGED: [char; 4] = ['\u{E4E6}', '\u{E4E7}', '\u{E4E8}', '\u{E4E9}'];

/// Height of the bar label above the staff.
const LABEL_HEIGHT: f32 = 16.0;
/// Stem length, in staff spaces.
const STEM: f32 = 3.5;

/// How a score looks. The glyphs' size comes from the staff spacing.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Under the fixed clef and key, and the line at their right edge.
    pub background: Color32,
    pub lead_edge: Color32,
    /// Staff lines, and the clef, key and time signature.
    pub staff: Color32,
    pub lead: Color32,
    pub bar_line: Color32,
    /// Notes, and those of unknown pitch, and those that clash with the chord.
    pub note: Color32,
    pub unknown: Color32,
    pub clash: Color32,
    /// Chord-tone hints next to a note: fitting, and clashing; and how faint.
    pub fits: Color32,
    pub hint_alpha: f32,
    /// Rests, ledger lines, and bar / tuplet labels.
    pub rest: Color32,
    pub ledger: Color32,
    pub label_font: FontId,
    pub label: Color32,
}

impl From<&Tokens> for Theme {
    fn from(t: &Tokens) -> Self {
        Self {
            background: t.bg(),
            lead_edge: t.grid(),
            staff: t.dim(),
            lead: t.text(),
            bar_line: t.text(),
            note: t.text(),
            unknown: t.dim(),
            clash: t.red(),
            fits: t.green(),
            hint_alpha: 0.4,
            rest: t.dim(),
            ledger: t.dim(),
            label_font: t.mono(10.0),
            label: t.dim(),
        }
    }
}

/// Vertical layout of a staff: `lo..=hi` are the staff positions to make room for (see
/// [`bg_core::score::extent`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaffLayout {
    /// Distance between staff lines.
    pub spacing: f32,
    pub lo: i32,
    pub hi: i32,
    /// Leave room to name the bars above the staff.
    pub labels: bool,
}

impl StaffLayout {
    pub fn height(&self) -> f32 {
        self.label_height() + self.above() + 4.0 * self.spacing + self.below()
    }

    fn label_height(&self) -> f32 {
        if self.labels { LABEL_HEIGHT } else { 0.0 }
    }

    fn above(&self) -> f32 {
        ((self.hi - TOP_LINE).max(0) as f32 / 2.0 + 1.5) * self.spacing
    }

    fn below(&self) -> f32 {
        ((-self.lo).max(0) as f32 / 2.0 + 1.5) * self.spacing
    }

    /// Screen y of the bottom staff line within `rect`.
    fn bottom_line(&self, rect: Rect) -> f32 {
        rect.top() + self.label_height() + self.above() + 4.0 * self.spacing
    }
}

/// Room for a key signature's signs, in staff spaces.
fn key_width(key: Option<KeySignature>) -> f32 {
    match key {
        Some(k) if k.count > 0 => k.count as f32 * KEY_SIGN + KEY_GAP,
        _ => 0.0,
    }
}

/// Distance between the signs of a key signature, in staff spaces.
const KEY_SIGN: f32 = 1.25;
/// Space after the key signature, in staff spaces.
const KEY_GAP: f32 = 0.8;

/// Where the staff lines begin, left of the axis, to fit the clef, key and time
/// signature.
pub fn lead_width(spacing: f32, clef: bool, key: Option<KeySignature>) -> f32 {
    (if clef { 3.8 } else { 0.5 } + key_width(key) + 2.2) * spacing
}

/// Paints a clef and key signature over the left end of a staff whose notes scroll by
/// behind them, as in the live score.
pub fn paint_fixed_lead(
    p: &Painter,
    rect: Rect,
    layout: StaffLayout,
    key: Option<KeySignature>,
    theme: &Theme,
) {
    let width = (4.2 + key_width(key)) * layout.spacing;
    let lead = Rect::from_min_size(rect.min, egui::vec2(width, rect.height()));
    p.rect_filled(lead, 0.0, theme.background);
    let d = Draw {
        p,
        t: theme,
        sp: layout.spacing,
        bottom: layout.bottom_line(rect),
        axis: TimeAxis::new(Rangef::new(lead.left(), lead.right()), 0.0, 1.0),
    };
    d.staff(lead);
    d.lead(
        lead,
        Lead {
            clef: true,
            key,
            sig: None,
        },
    );
    p.vline(
        lead.right(),
        d.y(TOP_LINE)..=d.bottom,
        Stroke::new(1.0, theme.lead_edge),
    );
}

pub struct ScoreView<'a> {
    pub bars: &'a [ScoreBar],
    pub axis: TimeAxis,
    pub layout: StaffLayout,
    /// Draw the clef and/or time signature at the left end of the staff.
    pub lead: Option<Lead>,
}

#[derive(Clone, Copy, Debug)]
pub struct Lead {
    pub clef: bool,
    pub key: Option<KeySignature>,
    pub sig: Option<TimeSignature>,
}

impl ScoreView<'_> {
    /// Paints into `rect`, which should be as tall as the layout's height.
    pub fn paint(&self, p: &Painter, rect: Rect, theme: &Theme) {
        let d = Draw {
            p,
            t: theme,
            sp: self.layout.spacing,
            bottom: self.layout.bottom_line(rect),
            axis: self.axis,
        };
        // Chord tones are a hint, so they go behind everything else.
        self.bars.iter().for_each(|b| d.chord_marks(b));
        d.staff(rect);
        if let Some(lead) = self.lead {
            d.lead(rect, lead);
        }
        for (i, bar) in self.bars.iter().enumerate() {
            d.bar(bar, rect, i == 0, self.layout.labels);
            let next = self.bars.get(i + 1);
            d.events(bar, next, rect);
        }
    }
}

struct Draw<'a> {
    p: &'a Painter,
    t: &'a Theme,
    sp: f32,
    bottom: f32,
    axis: TimeAxis,
}

impl Draw<'_> {
    fn y(&self, pos: i32) -> f32 {
        self.bottom - pos as f32 * self.sp / 2.0
    }

    /// Draws a glyph with its origin at `(x, y)`: SMuFL glyphs are designed around their
    /// baseline, at a font size of four staff spaces.
    fn glyph(&self, c: char, x: f32, y: f32, color: Color32) {
        let font = FontId::new(4.0 * self.sp, FontFamily::Name(MUSIC_FONT.into()));
        let galley = self.p.layout_no_wrap(c.to_string(), font, color);
        let baseline = galley
            .rows
            .first()
            .and_then(|r| r.row.glyphs.first().map(|g| r.pos.y + g.pos.y))
            .unwrap_or(0.0);
        self.p.galley(pos2(x, y - baseline), galley, color);
    }

    fn chord_marks(&self, bar: &ScoreBar) {
        let w = self.head_width();
        for e in &bar.events {
            let ScoreKind::Note(n) = &e.kind else {
                continue;
            };
            let x = self.head_x(e);
            let color = if n.clash { self.t.clash } else { self.t.fits };
            let faint = color.gamma_multiply(self.t.hint_alpha);
            let stroke = Stroke::new((0.3 * self.sp).max(2.0), faint);
            for &m in &n.chord_marks {
                self.p
                    .hline((x - 0.5 * w)..=(x + 1.5 * w), self.y(m), stroke);
            }
        }
    }

    fn staff(&self, rect: Rect) {
        let stroke = Stroke::new(1.0, self.t.staff);
        for line in 0..5 {
            self.p.hline(rect.x_range(), self.y(line * 2), stroke);
        }
    }

    fn lead(&self, rect: Rect, lead: Lead) {
        let color = self.t.lead;
        let mut x = rect.left() + 0.4 * self.sp;
        if lead.clef {
            self.glyph(F_CLEF, x, self.y(6), color);
            x += 3.6 * self.sp;
        }
        if let Some(key) = lead.key {
            let sign = if key.flats { FLAT } else { SHARP };
            for &pos in key.positions() {
                self.glyph(sign, x, self.y(pos), color);
                x += KEY_SIGN * self.sp;
            }
            if key.count > 0 {
                x += KEY_GAP * self.sp;
            }
        }
        let Some(sig) = lead.sig else {
            return;
        };
        for (n, pos) in [(sig.num, 6), (sig.den, 2)] {
            let mut digit_x = x;
            for c in n.to_string().chars() {
                let digit = c.to_digit(10).unwrap_or(0);
                let glyph = char::from_u32(SIG_ZERO + digit).unwrap_or(' ');
                self.glyph(glyph, digit_x, self.y(pos), color);
                digit_x += 1.6 * self.sp;
            }
        }
    }

    fn bar(&self, bar: &ScoreBar, rect: Rect, first: bool, labels: bool) {
        let lines = self.y(TOP_LINE)..=self.bottom;
        let stroke = Stroke::new(1.0, self.t.bar_line);
        let (left, right) = (self.axis.x(bar.start), self.axis.x(bar.end));
        if first {
            self.p.vline(left, lines.clone(), stroke);
        }
        self.p.vline(right, lines, stroke);
        if !labels {
            return;
        }
        let label = if bar.show_bpm {
            format!("BAR {} / {:.0} BPM", bar.index + 1, bar.bpm)
        } else {
            format!("BAR {}", bar.index + 1)
        };
        self.p.text(
            pos2(left + 2.0, rect.top()),
            Align2::LEFT_TOP,
            label,
            self.t.label_font.clone(),
            self.t.label,
        );
    }

    fn events(&self, bar: &ScoreBar, next: Option<&ScoreBar>, rect: Rect) {
        let e = &bar.events;
        self.beams(e);
        for (i, ev) in e.iter().enumerate() {
            match &ev.kind {
                ScoreKind::Note(n) => {
                    self.note(ev, n);
                    if n.tied {
                        let following = e[i + 1..]
                            .iter()
                            .chain(next.iter().flat_map(|b| b.events.iter()))
                            .find(|e| matches!(e.kind, ScoreKind::Note(_)));
                        let to = following.map_or(rect.right(), |f| self.head_x(f));
                        self.tie(self.head_x(ev), to, n);
                    }
                }
                ScoreKind::Rest => self.rest(ev),
                ScoreKind::BarRest => {
                    let x = (self.axis.x(bar.start) + self.axis.x(bar.end)) / 2.0;
                    self.glyph(REST_WHOLE, x - 0.6 * self.sp, self.y(6), self.t.rest);
                }
            }
        }
        self.tuplets(e);
    }

    fn head_x(&self, e: &ScoreEvent) -> f32 {
        self.axis.x(e.time) + 1.0
    }

    fn head_width(&self) -> f32 {
        1.18 * self.sp
    }

    fn pos(n: &ScoreNote) -> i32 {
        n.pitch.map_or(MIDDLE_LINE, |p| p.pos)
    }

    fn color(&self, n: &ScoreNote) -> Color32 {
        if n.clash {
            self.t.clash
        } else if n.pitch.is_none() {
            self.t.unknown
        } else {
            self.t.note
        }
    }

    fn note(&self, e: &ScoreEvent, n: &ScoreNote) {
        let x = self.head_x(e);
        let pos = Self::pos(n);
        let color = self.color(n);
        let w = self.head_width();

        for ledger in (2..=-pos)
            .step_by(2)
            .map(|l| -l)
            .chain((TOP_LINE + 2..=pos).step_by(2))
        {
            self.p.hline(
                (x - 0.3 * w)..=(x + 1.3 * w),
                self.y(ledger),
                Stroke::new(1.0, self.t.ledger),
            );
        }

        let head = match (n.pitch, e.value.denominator) {
            (None, _) => HEAD_X,
            (_, 1) => HEAD_WHOLE,
            (_, 2) => HEAD_HALF,
            _ => HEAD_BLACK,
        };
        self.glyph(head, x, self.y(pos), color);

        if !n.continuation
            && let Some(a) = n.pitch.and_then(|p| p.accidental)
        {
            let glyph = match a {
                Accidental::Sharp => SHARP,
                Accidental::Flat => FLAT,
                Accidental::Natural => NATURAL,
            };
            self.glyph(glyph, x - 1.1 * self.sp, self.y(pos), color);
        }

        for dot in 0..e.value.dots {
            let dot_pos = if pos.rem_euclid(2) == 0 { pos + 1 } else { pos };
            let dx = w + 0.4 * self.sp + dot as f32 * 0.5 * self.sp;
            self.p
                .circle_filled(pos2(x + dx, self.y(dot_pos)), 0.2 * self.sp, color);
        }

        if e.value.denominator >= 2 && n.beam_group.is_none() {
            let (stem_x, tip) = self.stem(x, pos, n.stem_up, STEM * self.sp);
            self.stem_line(stem_x, self.y(pos), tip, color);
            let flag = e.value.beams() as usize;
            if flag > 0 {
                let flags = if n.stem_up { FLAGS_UP } else { FLAGS_DOWN };
                self.glyph(flags[(flag - 1).min(3)], stem_x, tip, color);
            }
        }
    }

    fn stem(&self, head_x: f32, pos: i32, up: bool, len: f32) -> (f32, f32) {
        if up {
            (head_x + self.head_width() - 0.5, self.y(pos) - len)
        } else {
            (head_x + 0.5, self.y(pos) + len)
        }
    }

    fn stem_line(&self, x: f32, from: f32, to: f32, color: Color32) {
        self.p
            .line_segment([pos2(x, from), pos2(x, to)], Stroke::new(1.2, color));
    }

    /// Stems and beams of every beam group in a bar. Beams are flat, at the note furthest
    /// in the stem direction.
    fn beams(&self, events: &[ScoreEvent]) {
        let notes = || {
            events.iter().filter_map(|e| match &e.kind {
                ScoreKind::Note(n) => n.beam_group.map(|g| (g, e, n)),
                _ => None,
            })
        };
        let groups = notes().map(|(g, ..)| g).max().map_or(0, |g| g + 1);
        for group in 0..groups {
            let members: Vec<_> = notes().filter(|(g, ..)| *g == group).collect();
            let Some(&(_, _, first)) = members.first() else {
                continue;
            };
            let up = first.stem_up;
            let len = STEM * self.sp;
            let tips = members.iter().map(|(_, _, n)| self.y(Self::pos(n)));
            let tip = if up {
                tips.fold(f32::INFINITY, f32::min) - len
            } else {
                tips.fold(f32::NEG_INFINITY, f32::max) + len
            };
            let stems: Vec<f32> = members
                .iter()
                .map(|(_, e, n)| self.stem(self.head_x(e), Self::pos(n), up, len).0)
                .collect();
            let any_clash = members.iter().any(|(_, _, n)| n.clash);
            let beam_color = if any_clash { self.t.clash } else { self.t.note };
            for ((_, _, n), &x) in members.iter().zip(&stems) {
                self.stem_line(x, self.y(Self::pos(n)), tip, self.color(n));
            }
            let thick = 0.5 * self.sp;
            let inward = if up { 1.0 } else { -1.0 };
            for (k, pair) in members.windows(2).enumerate() {
                let shared = pair[0].1.value.beams().min(pair[1].1.value.beams());
                for level in 0..shared {
                    let y = tip + inward * level as f32 * 0.75 * self.sp;
                    let (y0, y1) = if up { (y, y + thick) } else { (y - thick, y) };
                    self.p.rect_filled(
                        Rect::from_x_y_ranges(stems[k]..=stems[k + 1], y0..=y1),
                        0.0,
                        beam_color,
                    );
                }
            }
        }
    }

    fn rest(&self, e: &ScoreEvent) {
        let x = self.head_x(e);
        let color = self.t.rest;
        let glyph = match e.value.denominator {
            1 => REST_WHOLE,
            2 => REST_HALF,
            4 => REST_QUARTER,
            _ => RESTS_FLAGGED[(e.value.beams() as usize).saturating_sub(1).min(3)],
        };
        let pos = if e.value.denominator == 1 {
            6
        } else {
            MIDDLE_LINE
        };
        self.glyph(glyph, x, self.y(pos), color);
        for dot in 0..e.value.dots {
            let dx = 1.5 * self.sp + dot as f32 * 0.5 * self.sp;
            self.p
                .circle_filled(pos2(x + dx, self.y(5)), 0.2 * self.sp, color);
        }
    }

    /// A curve between two noteheads, away from the stems.
    fn tie(&self, from: f32, to: f32, n: &ScoreNote) {
        let w = self.head_width();
        let side = if n.stem_up { 1.0 } else { -1.0 };
        let y = self.y(Self::pos(n)) + side * 0.7 * self.sp;
        let (x0, x1) = (from + w, to.max(from + w + 1.0));
        let points: Vec<Pos2> = (0..=12)
            .map(|i| {
                let f = i as f32 / 12.0;
                let bulge = 4.0 * f * (1.0 - f) * 0.5 * self.sp;
                pos2(x0 + (x1 - x0) * f, y + side * bulge)
            })
            .collect();
        self.p
            .add(Shape::line(points, Stroke::new(1.2, self.color(n))));
    }

    /// The tuplet number over each run of tuplet events within a beat.
    fn tuplets(&self, events: &[ScoreEvent]) {
        let mut i = 0;
        while i < events.len() {
            let Some(n) = events[i].tuplet else {
                i += 1;
                continue;
            };
            let beat = events[i].beat;
            let run = events[i..]
                .iter()
                .take_while(|e| e.tuplet == Some(n) && e.beat == beat)
                .count();
            let (a, b) = (self.head_x(&events[i]), self.head_x(&events[i + run - 1]));
            self.p.text(
                pos2((a + b) / 2.0 + 0.5 * self.sp, self.y(TOP_LINE) - 2.0),
                Align2::CENTER_BOTTOM,
                n.to_string(),
                self.t.label_font.clone(),
                self.t.label,
            );
            i += run;
        }
    }
}

/// A dotted vertical line, as used for expected note positions.
pub fn dotted_vline(p: &Painter, x: f32, y: std::ops::RangeInclusive<f32>, color: Color32) {
    let stroke = Stroke::new(1.0, color);
    let mut at = *y.start();
    while at < *y.end() {
        p.line_segment([pos2(x, at), pos2(x, (at + 2.0).min(*y.end()))], stroke);
        at += 5.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    #[test]
    fn low_notes_make_the_staff_taller() {
        let plain = StaffLayout {
            spacing: 8.0,
            lo: 0,
            hi: TOP_LINE,
            labels: true,
        };
        let low = StaffLayout { lo: -6, ..plain };
        let high = StaffLayout { hi: 14, ..plain };
        assert_eq!(low.height() - plain.height(), 3.0 * 8.0);
        assert_eq!(high.height() - plain.height(), 3.0 * 8.0);
    }

    #[test]
    fn labels_take_room_only_when_shown() {
        let with = StaffLayout {
            spacing: 8.0,
            lo: 0,
            hi: TOP_LINE,
            labels: true,
        };
        let without = StaffLayout {
            labels: false,
            ..with
        };
        assert_eq!(with.height() - without.height(), LABEL_HEIGHT);
    }

    #[test]
    fn lines_keep_their_place_within_the_rect() {
        let l = StaffLayout {
            spacing: 8.0,
            lo: -4,
            hi: TOP_LINE,
            labels: true,
        };
        let rect = Rect::from_min_size(pos2(0.0, 100.0), vec2(400.0, l.height()));
        let bottom = l.bottom_line(rect);
        assert!(bottom - 4.0 * 8.0 > rect.top() + LABEL_HEIGHT);
        assert!(bottom + 2.0 * 8.0 < rect.bottom());
    }
}
