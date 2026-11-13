//! A bass-clef score of what was played: turns matched and extra notes into bars of
//! notes, rests and ties for display. Pure layout data, shared by the results view and
//! the live score above the beat grid.
//!
//! Notes sit on a musical grid, not at their played time. A matched note starts at its
//! target; other notes round to the simplest subdivision of their beat that fits what
//! was played there, whatever the exercise's own grid. Lengths come from the playing:
//! a note held for most of the way to the next one reaches it, so a slight gap isn't
//! written as a rest. Bass is written an octave above sounding pitch.

use std::ops::Range;

use crate::harmony::Scale;
use crate::music::{TimeSignature, freq_to_midi};
use crate::plan::BarPlan;
use crate::scoring::{Grade, Matcher, Played, harmony_grade};

/// Diatonic number of the staff's bottom line (written G2).
const BOTTOM_LINE: i32 = 2 * 7 + 4;
/// Staff position of the middle line, counted in lines and spaces from the bottom line.
pub const MIDDLE_LINE: i32 = 4;
/// Staff position of the top line.
pub const TOP_LINE: i32 = 8;

/// Subdivisions of a beat, simplest first. Triplets come after sixteenths, so a loosely
/// played sixteenth doesn't read as a triplet.
const SUBDIVISIONS: [u32; 5] = [1, 2, 4, 3, 6];
/// How far from a subdivision a note may be and still fit it, as a part of the beat.
const FIT_TOLERANCE: f64 = 0.07;
/// A note held for this part of the way to the next one is taken as reaching it.
const LEGATO: f64 = 0.6;

const SHARP_LETTERS: [(i32, bool); 12] = [
    (0, false),
    (0, true),
    (1, false),
    (1, true),
    (2, false),
    (3, false),
    (3, true),
    (4, false),
    (4, true),
    (5, false),
    (5, true),
    (6, false),
];
const FLAT_LETTERS: [(i32, bool); 12] = [
    (0, false),
    (1, true),
    (1, false),
    (2, true),
    (2, false),
    (3, false),
    (4, true),
    (4, false),
    (5, true),
    (5, false),
    (6, true),
    (6, false),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accidental {
    Sharp,
    Flat,
    Natural,
}

impl Accidental {
    /// The sign that sets a note to `alter` semitones from its letter.
    fn of(alter: i8) -> Option<Self> {
        match alter {
            1 => Some(Self::Sharp),
            -1 => Some(Self::Flat),
            0 => Some(Self::Natural),
            _ => None,
        }
    }
}

/// A note's place on the staff: `pos` counts lines and spaces up from the bottom line
/// (0 = bottom line, 1 = first space, 8 = top line; negative = below the staff).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaffPitch {
    pub pos: i32,
    /// Semitones from the natural letter: -1 flat, 0 natural, 1 sharp.
    pub alter: i8,
    /// The sign written before the note. [`staff_pitch`] and [`Spelling::pitch`] mark
    /// every altered note; [`build`] then writes only what the key and bar call for.
    pub accidental: Option<Accidental>,
}

/// Pitch class of each natural letter, C first.
const NATURAL_PC: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// How notes are spelled: a letter (0 = C) and alteration per pitch class. In a key, its
/// own notes take its letters, and the others are sharps or flats as its signature is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spelling {
    names: [(i32, i8); 12],
}

impl Spelling {
    /// Every altered note as a sharp, or as a flat.
    pub fn chromatic(flats: bool) -> Self {
        let table = if flats { &FLAT_LETTERS } else { &SHARP_LETTERS };
        let mut names = [(0, 0); 12];
        for (name, &(letter, altered)) in names.iter_mut().zip(table) {
            let alter = match (altered, flats) {
                (false, _) => 0,
                (true, true) => -1,
                (true, false) => 1,
            };
            *name = (letter, alter);
        }
        Self { names }
    }

    /// The spelling of `scale`, or sharps without one.
    pub fn new(scale: Option<Scale>) -> Self {
        let Some(scale) = scale else {
            return Self::chromatic(false);
        };
        let mut out = Self::chromatic(scale.prefers_flats());
        let root_letter = out.names[(scale.root % 12) as usize].0;
        for (i, &pc) in scale.pitch_classes().iter().enumerate() {
            let letter = (root_letter + i as i32) % 7;
            let alter = (pc as i32 - NATURAL_PC[letter as usize] + 6).rem_euclid(12) - 6;
            if alter.abs() <= 1 {
                out.names[pc as usize] = (letter, alter as i8);
            }
        }
        out
    }

    /// Where a sounding MIDI note is written (an octave up), with its accidental.
    pub fn pitch(&self, sounding_midi: i32) -> StaffPitch {
        let written = sounding_midi + 12;
        let (letter, alter) = self.names[written.rem_euclid(12) as usize];
        // The octave of the letter: C flat belongs to the octave above its B.
        let octave = (written - alter as i32).div_euclid(12) - 1;
        StaffPitch {
            pos: octave * 7 + letter - BOTTOM_LINE,
            alter,
            accidental: (alter != 0).then(|| Accidental::of(alter)).flatten(),
        }
    }

    /// The name of pitch class `pc` as spelled: "C", "F#", "Bb", "Cb".
    pub fn name(&self, pc: u8) -> String {
        let (letter, alter) = self.names[(pc % 12) as usize];
        let sign = match alter {
            1 => "#",
            -1 => "b",
            _ => "",
        };
        format!("{}{sign}", "CDEFGAB".as_bytes()[letter as usize] as char)
    }
}

/// Where a note still to be played (Basic scales' upcoming targets) is written: alone,
/// so it carries the sign it needs against the key signature (a natural included) when
/// it isn't one of the key's notes, and none when it is.
pub fn target_pitch(sounding_midi: i32, scale: Option<Scale>) -> StaffPitch {
    let mut p = Spelling::new(scale).pitch(sounding_midi);
    let in_key = scale.is_some_and(|s| s.contains(sounding_midi.rem_euclid(12) as u8));
    p.accidental = match scale {
        Some(_) if in_key => None,
        Some(_) => Accidental::of(p.alter),
        None => (p.alter != 0).then(|| Accidental::of(p.alter)).flatten(),
    };
    p
}

/// Where a sounding MIDI note is written, every altered note with its accidental;
/// `flats` picks the spelling.
pub fn staff_pitch(sounding_midi: i32, flats: bool) -> StaffPitch {
    Spelling::chromatic(flats).pitch(sounding_midi)
}

/// Letters of the sharps in a key signature, in order (F C G D A E B), and their staff
/// positions on the bass clef.
const SHARP_LETTERS_ORDER: [i32; 7] = [3, 0, 4, 1, 5, 2, 6];
const SHARP_POSITIONS: [i32; 7] = [6, 3, 7, 4, 1, 5, 2];
/// The same for flats (B E A D G C F).
const FLAT_LETTERS_ORDER: [i32; 7] = [6, 2, 5, 1, 4, 0, 3];
const FLAT_POSITIONS: [i32; 7] = [2, 5, 1, 4, 0, 3, -1];

/// A key signature: `count` sharps or flats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeySignature {
    pub flats: bool,
    pub count: u8,
}

impl KeySignature {
    pub fn of(scale: &Scale) -> Self {
        let (flats, count) = match scale.parent_major() {
            0 => (false, 0),
            7 => (false, 1),
            2 => (false, 2),
            9 => (false, 3),
            4 => (false, 4),
            11 => (false, 5),
            5 => (true, 1),
            10 => (true, 2),
            3 => (true, 3),
            8 => (true, 4),
            1 => (true, 5),
            _ => (true, 6),
        };
        Self { flats, count }
    }

    /// Staff positions of the signs, in the order they're written.
    pub fn positions(&self) -> &'static [i32] {
        let all = if self.flats {
            &FLAT_POSITIONS
        } else {
            &SHARP_POSITIONS
        };
        &all[..self.count as usize]
    }

    /// The alteration the signature gives `letter` (0 = C).
    fn alter(&self, letter: i32) -> i8 {
        let order = if self.flats {
            &FLAT_LETTERS_ORDER
        } else {
            &SHARP_LETTERS_ORDER
        };
        match (order[..self.count as usize].contains(&letter), self.flats) {
            (false, _) => 0,
            (true, true) => -1,
            (true, false) => 1,
        }
    }
}

/// Writes only the accidentals a bar needs: a note's sign shows where it differs from
/// the key signature or from an earlier sign on the same line or space in the bar, which
/// then holds to the barline. Tied continuations carry none.
fn apply_key(events: &mut [ScoreEvent], key: Option<KeySignature>) {
    let from_key = |pos: i32| key.map_or(0, |k| k.alter((pos + BOTTOM_LINE).rem_euclid(7)));
    let mut changed: Vec<(i32, i8)> = Vec::new();
    for e in events {
        let ScoreKind::Note(n) = &mut e.kind else {
            continue;
        };
        let Some(p) = n.pitch.as_mut() else {
            continue;
        };
        if n.continuation {
            p.accidental = None;
            continue;
        }
        let current = changed
            .iter()
            .find(|c| c.0 == p.pos)
            .map_or_else(|| from_key(p.pos), |c| c.1);
        if p.alter == current {
            p.accidental = None;
        } else {
            p.accidental = Accidental::of(p.alter);
            changed.retain(|c| c.0 != p.pos);
            changed.push((p.pos, p.alter));
        }
    }
}

/// A chord in effect over a span of transport time. A neutral form of whatever the
/// exercise tracks, with `tones` as pitch classes, root first.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreChord {
    pub start: f64,
    pub end: f64,
    pub tones: Vec<u8>,
    pub label: String,
}

/// A written duration: 1 = whole, 4 = quarter, 8 = eighth, ...
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteValue {
    pub denominator: u32,
    pub dots: u8,
}

impl NoteValue {
    /// Flags or beams this value carries.
    pub fn beams(&self) -> u32 {
        (self.denominator.max(4).ilog2()) - 2
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoreNote {
    /// Index into `Matcher::played`.
    pub played: usize,
    /// `None` when no pitch was detected: drawn with a neutral notehead.
    pub pitch: Option<StaffPitch>,
    /// Clashes with the chord in effect.
    pub clash: bool,
    /// Staff positions of the chord tones around this note.
    pub chord_marks: Vec<i32>,
    /// Tied to the next note segment.
    pub tied: bool,
    /// Continues a tie from the previous segment, so it repeats no accidental.
    pub continuation: bool,
    pub stem_up: bool,
    /// Notes sharing a group number (within a bar) are beamed together.
    pub beam_group: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScoreKind {
    Note(ScoreNote),
    Rest,
    /// A whole bar of rest, drawn centred whatever the signature.
    BarRest,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoreEvent {
    /// Transport time of the grid position this sits on.
    pub time: f64,
    /// Beat within the bar.
    pub beat: u32,
    pub value: NoteValue,
    /// Notes per beat of the tuplet this belongs to, if any.
    pub tuplet: Option<u32>,
    pub kind: ScoreKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoreBar {
    pub index: i64,
    pub start: f64,
    pub end: f64,
    pub bpm: f32,
    /// First bar, or the tempo changed here.
    pub show_bpm: bool,
    pub sig: TimeSignature,
    pub events: Vec<ScoreEvent>,
}

pub struct ScoreInput<'a> {
    pub bars: &'a [BarPlan],
    pub matcher: &'a Matcher,
    pub chords: &'a [ScoreChord],
    pub scale: Option<Scale>,
    /// Reference tuning, in Hz.
    pub a4: f32,
    /// Nothing is drawn as a rest from here on (the current time in a live view;
    /// infinity once the session is over).
    pub until: f64,
}

/// A played note on the grid, in ticks counted from the first bar.
struct GridNote {
    start: u32,
    len: u32,
    played: usize,
}

/// The grid of the bars being laid out. Beats are counted from the first bar, and each
/// holds `tpb` ticks, enough for every subdivision in use.
struct Grid<'a> {
    bars: Vec<&'a BarPlan>,
    /// First beat of each bar, then the total.
    first_beat: Vec<u32>,
    tpb: u32,
    /// Finest straight division of a beat.
    straight: u32,
    /// The subdivision each beat is written in.
    subdivisions: Vec<u32>,
}

impl Grid<'_> {
    fn total_beats(&self) -> u32 {
        self.first_beat.last().copied().unwrap_or(0)
    }

    fn total_ticks(&self) -> u32 {
        self.total_beats() * self.tpb
    }

    fn is_tuplet(&self, beat: u32) -> bool {
        self.subdivisions
            .get(beat as usize)
            .is_some_and(|d| !d.is_power_of_two())
    }

    /// Ticks per grid position around `tick`.
    fn resolution(&self, tick: u32) -> u32 {
        let beat = tick / self.tpb;
        if self.is_tuplet(beat) {
            self.tpb / self.subdivisions[beat as usize]
        } else {
            self.tpb / self.straight
        }
    }
}

fn beat_len(bar: &BarPlan) -> f64 {
    bar.duration / bar.beats.max(1) as f64
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The subdivision that best suits a beat: `loose` are the positions (0..1) of notes to
/// round to it, `exact` the ticks of notes it must hold as they are. A lone note can't
/// establish a tuplet.
fn subdivision(loose: &[f64], exact: &[u32], tpb: u32, steps_per_beat: u32) -> u32 {
    if loose.iter().all(|f| f.abs() < FIT_TOLERANCE) && exact.iter().all(|&t| t == 0) {
        return 1;
    }
    let several = loose.len() + exact.len() >= 2;
    let usable = |d: &u32| {
        tpb.is_multiple_of(*d)
            && exact.iter().all(|t| t % (tpb / d) == 0)
            && (d.is_power_of_two() || *d == steps_per_beat || several)
    };
    let error = |d: u32| {
        let d = d as f64;
        loose
            .iter()
            .map(|f| (f - (f * d).round() / d).abs())
            .fold(0.0, f64::max)
    };
    let choices: Vec<u32> = std::iter::once(steps_per_beat)
        .chain(SUBDIVISIONS)
        .filter(usable)
        .collect();
    choices
        .iter()
        .copied()
        .find(|&d| error(d) <= FIT_TOLERANCE)
        .or_else(|| {
            choices
                .iter()
                .copied()
                .min_by(|&a, &b| error(a).total_cmp(&error(b)))
        })
        .unwrap_or(steps_per_beat)
}

/// A played note before it's on the grid.
struct Placing {
    played: usize,
    /// Set when it starts at a target; otherwise it rounds to its beat's subdivision.
    tick: Option<u32>,
    beat: u32,
    /// Position within `beat`.
    frac: f64,
    /// Length in beats, if known.
    beats: Option<f64>,
}

/// Works out which beat each note belongs to.
fn placings(input: &ScoreInput, grid: &Grid) -> Vec<Placing> {
    let (Some(first), Some(last)) = (grid.bars.first(), grid.bars.last()) else {
        return Vec::new();
    };
    let m = input.matcher;
    let window = m.thresholds.window_ms as f64 / 1000.0;
    let from = m.played.partition_point(|p| p.onset < first.start - window);
    let to = m.played.partition_point(|p| p.onset < last.end() + window);
    let mut out = Vec::new();
    for (played, p) in m.played.iter().enumerate().take(to).skip(from) {
        // In a live view a note waits for its pitch, which comes shortly after the onset.
        if input.until.is_finite() && p.freq.is_none() && p.offset.is_none() {
            continue;
        }
        let held = p.offset.map(|o| o - p.onset);
        if let Some(e) = p.matched.and_then(|e| m.expected.get(e)) {
            let Some(b) = grid.bars.iter().position(|b| b.index == e.bar) else {
                continue;
            };
            let bar = grid.bars[b];
            let len = beat_len(bar);
            let spb = bar.steps_per_beat.max(1) as f64;
            let step = (((e.time - bar.start) / len) * spb).round();
            let tick = grid.first_beat[b] * grid.tpb + (step * grid.tpb as f64 / spb) as u32;
            out.push(Placing {
                played,
                tick: Some(tick),
                beat: tick / grid.tpb,
                frac: 0.0,
                beats: Some(held.unwrap_or(e.len) / len),
            });
        } else {
            let b = grid
                .bars
                .iter()
                .rposition(|b| b.start <= p.onset)
                .unwrap_or_default();
            let bar = grid.bars[b];
            let len = beat_len(bar);
            let pos = grid.first_beat[b] as f64 + (p.onset - bar.start) / len;
            let beat = (pos + FIT_TOLERANCE)
                .floor()
                .clamp(0.0, grid.total_beats().saturating_sub(1) as f64);
            out.push(Placing {
                played,
                tick: None,
                beat: beat as u32,
                frac: pos - beat,
                beats: held.map(|h| h / len),
            });
        }
    }
    out
}

/// Places every played note on the grid, one voice: a note ends where the next starts.
fn grid_notes(placings: &[Placing], grid: &mut Grid) -> Vec<GridNote> {
    let tpb = grid.tpb;
    let spb = grid.bars.first().map_or(1, |b| b.steps_per_beat.max(1));
    for (beat, d) in grid.subdivisions.iter_mut().enumerate() {
        let here = || placings.iter().filter(|p| p.beat as usize == beat);
        let loose: Vec<f64> = here()
            .filter(|p| p.tick.is_none())
            .map(|p| p.frac)
            .collect();
        let exact: Vec<u32> = here().filter_map(|p| p.tick).map(|t| t % tpb).collect();
        *d = subdivision(&loose, &exact, tpb, spb);
    }

    let mut notes: Vec<GridNote> = placings
        .iter()
        .filter_map(|p| {
            p.tick.map(|start| GridNote {
                start,
                len: 1,
                played: p.played,
            })
        })
        .collect();
    notes.sort_by_key(|n| n.start);
    let total = grid.total_ticks();
    // In playing order, so a note can't end up before one played earlier.
    let mut previous = None;
    for (i, p) in placings.iter().enumerate() {
        if let Some(tick) = p.tick {
            previous = Some(tick);
            continue;
        }
        let d = grid.subdivisions[p.beat as usize];
        let position = (p.frac * d as f64).round();
        let played_late = p.frac * d as f64 > position;
        let ideal = p.beat * tpb + position.max(0.0) as u32 * (tpb / d);
        let before = placings[i + 1..]
            .iter()
            .find_map(|p| p.tick)
            .unwrap_or(total);
        let spot = Spot {
            after: previous,
            before,
            ideal: ideal.min(total.saturating_sub(1)),
            played_late,
        };
        let Some(start) = spot.choose(&notes, grid) else {
            continue;
        };
        previous = Some(start);
        let at = notes.partition_point(|n| n.start <= start);
        notes.insert(
            at,
            GridNote {
                start,
                len: 1,
                played: p.played,
            },
        );
    }

    for i in 0..notes.len() {
        let start = notes[i].start;
        let following = notes[i + 1..].iter().find(|n| n.start > start);
        let room = following.map_or(total - start, |n| n.start - start);
        let beats = placings
            .iter()
            .find(|p| p.played == notes[i].played)
            .and_then(|p| p.beats);
        notes[i].len = length(grid, start, beats, room, following.is_some());
    }
    notes
}

/// A note's length in ticks: `beats` as played, within the `room` up to the next note.
fn length(grid: &Grid, start: u32, beats: Option<f64>, room: u32, followed: bool) -> u32 {
    let shortest = grid.resolution(start);
    let Some(beats) = beats else {
        return shortest.min(room).max(1);
    };
    let ticks = beats * grid.tpb as f64;
    if followed && ticks >= LEGATO * room as f64 {
        return room;
    }
    let units = ticks / shortest as f64;
    let n = if grid.is_tuplet(start / grid.tpb) {
        units.round().max(1.0)
    } else if units < 1.5 {
        1.0
    } else {
        // Sixteenths are for short notes; anything longer reads better in eighths.
        (units / 2.0).round() * 2.0
    };
    let end = start + n as u32 * shortest;
    // Whatever the note ends in has to be able to hold that position.
    let at = grid.resolution(end.min(grid.total_ticks().saturating_sub(1)));
    (end.div_ceil(at) * at - start).min(room).max(1)
}

/// Where a note without a target may go: between its neighbours in playing order.
struct Spot {
    after: Option<u32>,
    before: u32,
    ideal: u32,
    played_late: bool,
}

impl Spot {
    /// The tick for the note. If `ideal` is taken it moves by the smallest step on the
    /// side it was played on, then the other. A note with nowhere to go overlaps.
    fn choose(&self, notes: &[GridNote], grid: &Grid) -> Option<u32> {
        let fits = |t: u32| {
            t < self.before
                && self.after.is_none_or(|a| t > a)
                && !notes.iter().any(|n| n.start == t)
        };
        if fits(self.ideal) {
            return Some(self.ideal);
        }
        let later = self.ideal + grid.resolution(self.ideal);
        let earlier = self
            .ideal
            .checked_sub(grid.resolution(self.ideal.saturating_sub(1)));
        let sides = if self.played_late {
            [Some(later), earlier]
        } else {
            [earlier, Some(later)]
        };
        let found = sides.into_iter().flatten().find(|&t| fits(t));
        Some(found.unwrap_or(self.ideal))
    }
}

/// The notes and rests of bar number `b`, including parts of notes tied in from earlier
/// bars.
fn bar_events(
    b: usize,
    grid: &Grid,
    notes: &[GridNote],
    input: &ScoreInput,
    spelling: &Spelling,
) -> Vec<ScoreEvent> {
    let bar = grid.bars[b];
    let tpb = grid.tpb;
    let first = grid.first_beat[b] * tpb;
    let last = grid.first_beat[b + 1] * tpb;
    let len = beat_len(bar);
    let time = |tick: u32| bar.start + (tick - first) as f64 / tpb as f64 * len;
    let beat = |tick: u32| (tick - first) / tpb;
    let mut events = Vec::new();
    let mut cursor = first;
    // A rest first finishes its beat, so the beats stay readable.
    let rest = |events: &mut Vec<ScoreEvent>, from: u32, to: u32| {
        let next_beat = from.div_ceil(tpb) * tpb;
        let parts = [(from, next_beat.min(to)), (next_beat.min(to), to)];
        for (a, b) in parts.into_iter().filter(|(a, b)| a < b) {
            for (tick, value, tuplet) in split(grid, bar, a, b) {
                if time(tick) < input.until {
                    events.push(ScoreEvent {
                        time: time(tick),
                        beat: beat(tick),
                        value,
                        tuplet,
                        kind: ScoreKind::Rest,
                    });
                }
            }
        }
    };
    let in_bar = notes
        .iter()
        .filter(|n| n.start < last && n.start + n.len > first);
    for n in in_bar {
        let from = n.start.max(first);
        let to = (n.start + n.len).min(last);
        if from > cursor {
            rest(&mut events, cursor, from);
        }
        let continues = n.start + n.len > last;
        let parts = split(grid, bar, from, to);
        let count = parts.len();
        // Judged under the chord where it's written (not where it was played), so a
        // note played just before a chord change and written after it fits that chord.
        let written = (n.start >= first).then(|| time(n.start));
        let note = score_note(input, n.played, written, spelling);
        for (k, (tick, value, tuplet)) in parts.into_iter().enumerate() {
            let continuation = k > 0 || n.start < first;
            events.push(ScoreEvent {
                time: time(tick),
                beat: beat(tick),
                value,
                tuplet,
                kind: ScoreKind::Note(ScoreNote {
                    tied: k + 1 < count || continues,
                    continuation,
                    chord_marks: if continuation {
                        Vec::new()
                    } else {
                        note.chord_marks.clone()
                    },
                    ..note.clone()
                }),
            });
        }
        cursor = cursor.max(to);
    }
    if events.is_empty() {
        if bar.start < input.until {
            events.push(ScoreEvent {
                time: bar.start,
                beat: 0,
                value: NoteValue {
                    denominator: 1,
                    dots: 0,
                },
                tuplet: None,
                kind: ScoreKind::BarRest,
            });
        }
    } else if cursor < last {
        rest(&mut events, cursor, last);
    }
    events
}

/// Builds the score. Count-in bars are left out. `bars` may be any contiguous part of
/// the session; notes outside it are skipped.
pub fn build(input: &ScoreInput) -> Vec<ScoreBar> {
    let bars: Vec<&BarPlan> = input.bars.iter().filter(|b| !b.is_count_in()).collect();
    let mut first_beat = Vec::with_capacity(bars.len() + 1);
    let mut total = 0;
    for b in &bars {
        first_beat.push(total);
        total += b.beats.max(1);
    }
    first_beat.push(total);
    let spb = bars.first().map_or(1, |b| b.steps_per_beat.max(1));
    let mut grid = Grid {
        bars,
        first_beat,
        tpb: 12 * spb / gcd(12, spb),
        straight: if spb.is_power_of_two() { spb.max(4) } else { 4 },
        subdivisions: vec![1; total as usize],
    };
    let placed = placings(input, &grid);
    let notes = grid_notes(&placed, &mut grid);

    let spelling = Spelling::new(input.scale);
    let key = input.scale.map(|s| KeySignature::of(&s));
    let mut out: Vec<ScoreBar> = Vec::with_capacity(grid.bars.len());
    let mut last_bpm = None;
    for (i, bar) in grid.bars.iter().enumerate() {
        let mut events = bar_events(i, &grid, &notes, input, &spelling);
        apply_key(&mut events, key);
        beam(&mut events);
        out.push(ScoreBar {
            index: bar.index,
            start: bar.start,
            end: bar.end(),
            bpm: bar.bpm,
            show_bpm: last_bpm != Some(bar.bpm),
            sig: bar.sig,
            events,
        });
        last_bpm = Some(bar.bpm);
    }
    out
}

/// `written`: the time the note is written at, when it starts in this bar.
fn score_note(
    input: &ScoreInput,
    played: usize,
    written: Option<f64>,
    spelling: &Spelling,
) -> ScoreNote {
    let m = input.matcher;
    let p: &Played = &m.played[played];
    let midi = p.freq.map(|f| freq_to_midi(f, input.a4).0);
    let pitch = midi.map(|n| spelling.pitch(n));
    let at = written.unwrap_or_else(|| {
        p.matched
            .and_then(|e| m.expected.get(e))
            .map_or(p.onset, |e| e.time)
    });
    let chord = input
        .chords
        .iter()
        .find(|c| c.start - 1e-6 <= at && at < c.end);
    let chord_marks = match (midi, chord) {
        (Some(n), Some(c)) => chord_marks(n, &c.tones, spelling),
        _ => Vec::new(),
    };
    ScoreNote {
        played,
        pitch,
        // A chord tone where it's written isn't a clash, whatever chord it was judged in.
        clash: p.harmony.map(harmony_grade) == Some(Grade::Poor)
            && !chord
                .zip(midi)
                .is_some_and(|(c, n)| c.tones.contains(&(n.rem_euclid(12) as u8))),
        chord_marks,
        tied: false,
        continuation: false,
        stem_up: pitch.map_or(MIDDLE_LINE, |p| p.pos) < MIDDLE_LINE,
        beam_group: None,
    }
}

/// Staff positions of `tones` (root first) stacked within an octave, from the nearest
/// root at or below the sounding note `midi`.
pub fn chord_marks(midi: i32, tones: &[u8], spelling: &Spelling) -> Vec<i32> {
    let Some(&root) = tones.first() else {
        return Vec::new();
    };
    let root = root as i32;
    let base = midi - (midi - root).rem_euclid(12);
    tones
        .iter()
        .map(|&t| spelling.pitch(base + (t as i32 - root).rem_euclid(12)).pos)
        .collect()
}

/// Splits ticks `start..end` of `bar` into written values: `(tick, value, tuplet)`. Parts
/// of tuplet beats are written as tuplets; everything else, whole tuplet beats included,
/// plainly.
fn split(grid: &Grid, bar: &BarPlan, start: u32, end: u32) -> Vec<(u32, NoteValue, Option<u32>)> {
    let tpb = grid.tpb;
    let den = bar.sig.den.max(1);
    let plain_at =
        |pos: u32| !grid.is_tuplet(pos / tpb) || (pos.is_multiple_of(tpb) && end - pos >= tpb);
    let mut out = Vec::new();
    let mut pos = start;
    while pos < end {
        if plain_at(pos) {
            let mut run = pos;
            while run < end && plain_at(run) {
                run = ((run / tpb + 1) * tpb).min(end);
            }
            let unit = tpb / grid.straight;
            for (at, value) in binary((run - pos).div_ceil(unit), den * grid.straight) {
                out.push((pos + at * unit, value, None));
            }
            pos = run;
        } else {
            let d = grid.subdivisions[(pos / tpb) as usize];
            let part = ((pos / tpb + 1) * tpb).min(end) - pos;
            let unit = tpb / d;
            let base = 1 << d.ilog2();
            for (at, value) in binary(part.div_ceil(unit), den * base) {
                out.push((pos + at * unit, value, Some(d)));
            }
            pos += part;
        }
    }
    out
}

/// Splits `units` of 1/`unit` of a whole note into the fewest values, longest first:
/// `(offset in units, value)`.
fn binary(units: u32, unit: u32) -> Vec<(u32, NoteValue)> {
    let mut out = Vec::new();
    let (mut left, mut at) = (units, 0);
    while left > 0 {
        let plain = 1 << left.ilog2().min(unit.ilog2());
        let dotted = plain + plain / 2;
        let (take, base, dots) = if plain >= 2 && dotted <= left && plain < unit {
            (dotted, plain, 1)
        } else {
            (plain, plain, 0)
        };
        out.push((
            at,
            NoteValue {
                denominator: unit / base,
                dots,
            },
        ));
        left -= take;
        at += take;
    }
    out
}

/// Beams runs of flagged notes within a beat, with one stem direction per group.
fn beam(events: &mut [ScoreEvent]) {
    let mut group = 0;
    let mut i = 0;
    while i < events.len() {
        let beamable = |e: &ScoreEvent| matches!(e.kind, ScoreKind::Note(_)) && e.value.beams() > 0;
        let mut end = i;
        while end < events.len() && beamable(&events[end]) && events[end].beat == events[i].beat {
            end += 1;
        }
        if end - i >= 2 {
            let notes = || {
                events[i..end].iter().filter_map(|e| match &e.kind {
                    ScoreKind::Note(n) => Some(n.pitch.map_or(MIDDLE_LINE, |p| p.pos)),
                    _ => None,
                })
            };
            let far = notes()
                .max_by_key(|p| (p - MIDDLE_LINE).abs())
                .unwrap_or(MIDDLE_LINE);
            for e in &mut events[i..end] {
                if let ScoreKind::Note(n) = &mut e.kind {
                    n.beam_group = Some(group);
                    n.stem_up = far < MIDDLE_LINE;
                }
            }
            group += 1;
        }
        i = end.max(i + 1);
    }
}

/// Bars per row for a row `width` wide, keeping the densest bar readable at a staff
/// line `spacing` (wider spacing, bigger notes, fewer bars).
pub fn bars_per_row(bars: &[ScoreBar], width: f32, spacing: f32) -> usize {
    let densest = bars.iter().map(|b| b.events.len()).max().unwrap_or(0);
    let scale = (spacing / 8.0).max(0.1);
    let bar_width = (densest as f32 * 40.0 + 40.0).max(220.0) * scale;
    ((width / bar_width) as usize).max(1)
}

/// Splits `count` bars into rows of `per_row`.
pub fn rows(count: usize, per_row: usize) -> Vec<Range<usize>> {
    let per_row = per_row.max(1);
    (0..count)
        .step_by(per_row)
        .map(|s| s..(s + per_row).min(count))
        .collect()
}

/// Lowest and highest staff positions used, always covering the staff itself.
pub fn extent(bars: &[ScoreBar]) -> (i32, i32) {
    let mut lo = 0;
    let mut hi = TOP_LINE;
    for b in bars {
        for e in &b.events {
            if let ScoreKind::Note(n) = &e.kind {
                for pos in n
                    .pitch
                    .iter()
                    .map(|p| p.pos)
                    .chain(n.chord_marks.iter().copied())
                {
                    lo = lo.min(pos);
                    hi = hi.max(pos);
                }
            }
        }
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harmony::Mode;
    use crate::music::midi_to_freq;
    use crate::scoring::ScoringThresholds;

    const A4: f32 = 440.0;

    /// Bars of 2 s (120 BPM, 4/4) from time 0, with `notes` as (step, steps) in each.
    fn bars(count: i64, spb: u32, notes: &[(u32, u32)]) -> Vec<BarPlan> {
        (0..count)
            .map(|index| {
                let start = index as f64 * 2.0;
                let step = 0.5 / spb as f64;
                BarPlan {
                    index,
                    start,
                    duration: 2.0,
                    bpm: 120.0,
                    beats: 4,
                    sig: TimeSignature::default(),
                    steps_per_beat: spb,
                    muted: false,
                    clicks: Vec::new(),
                    notes: notes
                        .iter()
                        .map(|&(s, l)| crate::plan::PlannedNote {
                            time: start + s as f64 * step,
                            len: l as f64 * step,
                            accent: false,
                        })
                        .collect(),
                }
            })
            .collect()
    }

    fn matcher(bars: &[BarPlan]) -> Matcher {
        let mut m = Matcher::new(ScoringThresholds::default());
        bars.iter().for_each(|b| m.add_bar(b));
        m
    }

    fn play(m: &mut Matcher, onset: f64, offset: f64, midi: i32) -> usize {
        let i = m.on_onset(onset, -10.0);
        m.on_offset(i, offset, -10.0, Some(midi_to_freq(midi as f32, A4)));
        i
    }

    fn score(bars: &[BarPlan], m: &Matcher) -> Vec<ScoreBar> {
        build(&ScoreInput {
            bars,
            matcher: m,
            chords: &[],
            scale: None,
            a4: A4,
            until: f64::INFINITY,
        })
    }

    /// (time in ms, denominator, dots, is a note) of each event.
    fn values(bar: &ScoreBar) -> Vec<(u32, u32, u8, bool)> {
        bar.events
            .iter()
            .map(|e| {
                let note = matches!(e.kind, ScoreKind::Note(_));
                let ms = (e.time * 1000.0).round() as u32;
                (ms, e.value.denominator, e.value.dots, note)
            })
            .collect()
    }

    /// Times in ms of the notes that start in `bar`.
    fn note_times(bar: &ScoreBar) -> Vec<u32> {
        bar.events
            .iter()
            .filter(|e| matches!(&e.kind, ScoreKind::Note(n) if !n.continuation))
            .map(|e| (e.time * 1000.0).round() as u32)
            .collect()
    }

    fn note(e: &ScoreEvent) -> &ScoreNote {
        match &e.kind {
            ScoreKind::Note(n) => n,
            k => panic!("not a note: {k:?}"),
        }
    }

    #[test]
    fn bass_is_written_an_octave_up() {
        // Open E sits on the first ledger line below the staff, open G in the top space.
        assert_eq!(staff_pitch(28, false).pos, -2);
        assert_eq!(staff_pitch(43, false).pos, 7);
        // G1 is on the bottom line.
        assert_eq!(staff_pitch(31, false).pos, 0);
        // Open B on a five-string: below the second ledger line.
        assert_eq!(staff_pitch(23, false).pos, -5);
    }

    #[test]
    fn spelling_follows_the_key() {
        let sharp = staff_pitch(30, false); // F#1, written F#2: the space below the staff.
        assert_eq!(sharp.pos, -1);
        assert_eq!(sharp.accidental, Some(Accidental::Sharp));
        let flat = staff_pitch(30, true); // Gb, on the bottom line.
        assert_eq!(flat.pos, 0);
        assert_eq!(flat.accidental, Some(Accidental::Flat));
        assert_eq!(staff_pitch(28, true).accidental, None);
        let f_major = Scale {
            root: 5,
            mode: Mode::Ionian,
        };
        let e_major = Scale {
            root: 4,
            mode: Mode::Ionian,
        };
        let d_dorian = Scale {
            root: 2,
            mode: Mode::Dorian,
        };
        let g_dorian = Scale {
            root: 7,
            mode: Mode::Dorian,
        };
        assert!(f_major.prefers_flats());
        assert!(!e_major.prefers_flats());
        assert!(!d_dorian.prefers_flats()); // Parent C major.
        assert!(g_dorian.prefers_flats()); // Parent F major.
    }

    fn accidentals(bar: &ScoreBar) -> Vec<Option<Accidental>> {
        bar.events
            .iter()
            .filter_map(|e| match &e.kind {
                ScoreKind::Note(n) => Some(n.pitch.unwrap().accidental),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn key_signatures_sit_on_the_bass_clef() {
        let key = |root, mode| KeySignature::of(&Scale { root, mode });
        let e = key(4, Mode::Ionian);
        assert_eq!((e.flats, e.positions()), (false, &[6, 3, 7, 4][..]));
        let b_flat = key(10, Mode::Ionian);
        assert_eq!((b_flat.flats, b_flat.positions()), (true, &[2, 5][..]));
        // A Dorian shares G major's single sharp.
        assert_eq!(key(9, Mode::Dorian).positions(), [6]);
        assert!(key(0, Mode::Ionian).positions().is_empty());
    }

    #[test]
    fn keys_spell_their_own_notes() {
        // F Locrian (parent G flat major) has a C flat: written on C's space.
        let f_locrian = Spelling::new(Some(Scale {
            root: 5,
            mode: Mode::Locrian,
        }));
        let c_flat = f_locrian.pitch(35);
        assert_eq!((c_flat.pos, c_flat.alter), (3, -1));
        assert_eq!(staff_pitch(35, false).pos, 2);
        // Outside the key, the signature's kind of sign: G sharp in E flat is A flat.
        let e_flat = Spelling::new(Some(Scale {
            root: 3,
            mode: Mode::Ionian,
        }));
        assert_eq!(e_flat.pitch(32).alter, -1);
        assert_eq!(f_locrian.name(11), "Cb");
        assert_eq!(e_flat.name(8), "Ab");
        assert_eq!(Spelling::new(None).name(6), "F#");
    }

    #[test]
    fn targets_carry_only_the_signs_the_key_lacks() {
        let e = Some(Scale {
            root: 4,
            mode: Mode::Ionian,
        });
        // F sharp is in E major: no sign. D natural (E blues' b7) needs one.
        assert_eq!(target_pitch(30, e).accidental, None);
        assert_eq!(target_pitch(38, e).accidental, Some(Accidental::Natural));
        assert_eq!(target_pitch(34, e).accidental, Some(Accidental::Sharp));
        assert_eq!(target_pitch(30, None).accidental, Some(Accidental::Sharp));
        assert_eq!(target_pitch(31, None).accidental, None);
    }

    #[test]
    fn accidentals_follow_the_key_and_last_to_the_barline() {
        let b = bars(2, 1, &[(0, 1), (1, 1), (2, 1), (3, 1)]);
        let mut m = matcher(&b);
        // Bar 1: F#, F#, F, F#. Bar 2: F, F#, G, G.
        for (i, midi) in [42, 42, 41, 42, 41, 42, 43, 43].into_iter().enumerate() {
            let t = i as f64 * 0.5;
            play(&mut m, t, t + 0.45, midi);
        }
        let build_in = |scale| {
            build(&ScoreInput {
                bars: &b,
                matcher: &m,
                chords: &[],
                scale,
                a4: A4,
                until: f64::INFINITY,
            })
        };
        use Accidental::*;
        let c = build_in(Some(Scale {
            root: 0,
            mode: Mode::Ionian,
        }));
        assert_eq!(
            accidentals(&c[0]),
            [Some(Sharp), None, Some(Natural), Some(Sharp)]
        );
        assert_eq!(accidentals(&c[1]), [None, Some(Sharp), None, None]);
        // In G major F# is in the key: only the F naturals need a sign.
        let g = build_in(Some(Scale {
            root: 7,
            mode: Mode::Ionian,
        }));
        assert_eq!(accidentals(&g[0]), [None, None, Some(Natural), Some(Sharp)]);
        assert_eq!(accidentals(&g[1]), [Some(Natural), Some(Sharp), None, None]);
    }

    #[test]
    fn matched_notes_take_the_target_grid_position() {
        let b = bars(1, 1, &[(0, 1), (1, 1), (2, 1), (3, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.02, 0.38, 40); // Late.
        play(&mut m, 0.48, 0.9, 40); // Early.
        let s = score(&b, &m);
        assert_eq!(
            values(&s[0]),
            [
                (0, 4, 0, true),
                (500, 4, 0, true),
                (1000, 2, 0, false), // The two unplayed beats.
            ]
        );
        assert!((s[0].events[1].time - 0.5).abs() < 1e-9);
    }

    #[test]
    fn extra_notes_round_to_a_musical_grid() {
        let b = bars(1, 2, &[(0, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.76, 0.99, 40); // The off-beat of beat 2.
        assert_eq!(note_times(&score(&b, &m)[0]), [750]);
    }

    #[test]
    fn eighths_show_over_a_groove_of_quarters() {
        // As in a real report: the exercise wants quarters, the player plays eighths,
        // slightly unevenly and not quite held to the next note.
        let beat = 0.706;
        let b: Vec<BarPlan> = bars(1, 1, &[(0, 1), (1, 1), (2, 1), (3, 1)])
            .into_iter()
            .map(|mut bar| {
                bar.duration = beat * 4.0;
                for (i, n) in bar.notes.iter_mut().enumerate() {
                    n.time = i as f64 * beat;
                    n.len = beat;
                }
                bar
            })
            .collect();
        let mut m = matcher(&b);
        let onsets = [0.003, 0.343, 0.708, 1.056, 1.422, 1.787, 2.161, 2.511];
        for (i, &on) in onsets.iter().enumerate() {
            let next = onsets.get(i + 1).copied().unwrap_or(on + 0.35);
            play(&mut m, on, next - 0.05, 40);
        }
        let s = score(&b, &m);
        let eighths = s[0]
            .events
            .iter()
            .filter(|e| matches!(e.kind, ScoreKind::Note(_)) && e.value.denominator == 8)
            .count();
        assert_eq!(eighths, 8, "{:?}", values(&s[0]));
        assert!(
            !s[0]
                .events
                .iter()
                .any(|e| matches!(e.kind, ScoreKind::Rest))
        );
    }

    #[test]
    fn a_note_reaches_the_next_when_held_most_of_the_way() {
        let b = bars(1, 1, &[(0, 1), (1, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.0, 0.35, 40);
        play(&mut m, 0.5, 0.65, 40); // Short, with nothing after it.
        assert_eq!(
            values(&score(&b, &m)[0]),
            [
                (0, 4, 0, true),
                (500, 16, 0, true),
                (625, 8, 1, false), // Finishes the beat...
                (1000, 2, 0, false),
            ]
        );
    }

    #[test]
    fn sixteenths_and_triplets_are_told_apart() {
        let b = bars(1, 1, &[]);
        let mut m = matcher(&b);
        // Beat 1 in sixteenths, beat 2 in triplets, a little off in places.
        for t in [0.005, 0.13, 0.245, 0.38] {
            play(&mut m, t, t + 0.1, 40);
        }
        for t in [0.5, 0.67, 0.83] {
            play(&mut m, t, t + 0.14, 40);
        }
        let s = score(&b, &m);
        let e = &s[0].events;
        assert_eq!(note_times(&s[0]), [0, 125, 250, 375, 500, 667, 833]);
        assert!(
            e[..4]
                .iter()
                .all(|e| (e.value.denominator, e.tuplet) == (16, None))
        );
        assert!(
            e[4..7]
                .iter()
                .all(|e| (e.value.denominator, e.tuplet) == (8, Some(3)))
        );
    }

    /// Bars 6 and 7 of a real report at 70 BPM, quarters with a half note: a pitchless
    /// note (string noise, before the detector told it apart), then the note it preceded.
    fn noise_before_a_note() -> (Vec<BarPlan>, Matcher, usize, usize) {
        let beat = 60.0 / 70.0;
        let b: Vec<BarPlan> = bars(2, 1, &[(0, 1), (1, 1), (2, 2)])
            .into_iter()
            .enumerate()
            .map(|(i, mut bar)| {
                bar.start = 0.5 + i as f64 * beat * 4.0;
                bar.duration = beat * 4.0;
                bar.bpm = 70.0;
                bar.notes = [
                    (0.0, 1.0),
                    (1.0, 1.0),
                    (2.0, if i == 0 { 2.0 } else { 1.0 }),
                ]
                .iter()
                .map(|&(at, len)| crate::plan::PlannedNote {
                    time: bar.start + at * beat,
                    len: len * beat,
                    accent: false,
                })
                .collect();
                bar
            })
            .collect();
        let mut m = matcher(&b);
        play(&mut m, 0.484, 1.139, 38);
        play(&mut m, 1.292, 2.091, 41);
        play(&mut m, 2.188, 3.262, 43);
        let noise = m.on_onset(3.843, -40.0);
        m.on_offset(noise, 3.911, -40.0, None);
        let note = play(&mut m, 3.911, 4.668, 43);
        play(&mut m, 4.77, 5.484, 50);
        (b, m, noise, note)
    }

    #[test]
    fn a_pitchless_extra_makes_way_and_the_note_stays_on_its_beat() {
        let (b, m, ghost, real) = noise_before_a_note();
        let s = score(&b, &m);
        let played: Vec<usize> = s
            .iter()
            .flat_map(|b| &b.events)
            .filter_map(|e| match &e.kind {
                ScoreKind::Note(n) if !n.continuation => Some(n.played),
                _ => None,
            })
            .collect();
        // Ghost notes are written too (as x noteheads): the extra goes just before.
        assert!(played.contains(&ghost) && played.contains(&real));
        let ghost_at = s[0]
            .events
            .iter()
            .find(|e| matches!(&e.kind, ScoreKind::Note(n) if n.played == ghost))
            .unwrap();
        assert!(ghost_at.time > s[0].start && ghost_at.time < s[0].end);
        // Beat 1 of the second bar, not a beat before it.
        let first = s[1].events.first().unwrap();
        assert!((first.time - s[1].start).abs() < 1e-9);
        assert_eq!(note(first).played, real);
    }

    #[test]
    fn notes_keep_their_playing_order() {
        let (b, m, ..) = noise_before_a_note();
        let s = score(&b, &m);
        let played: Vec<usize> = s
            .iter()
            .flat_map(|b| &b.events)
            .filter_map(|e| match &e.kind {
                ScoreKind::Note(n) if !n.continuation => Some(n.played),
                _ => None,
            })
            .collect();
        assert!(played.is_sorted(), "{played:?}");
    }

    #[test]
    fn a_pitched_extra_moves_by_the_smallest_step() {
        let b = bars(1, 1, &[(1, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.5, 0.9, 40); // On the target at beat 2.
        // Just after it and outside its reach, rounding onto the same beat.
        let i = m.on_onset(0.52, -10.0);
        m.on_offset(i, 0.9, -10.0, Some(midi_to_freq(43.0, A4)));
        assert_eq!(note_times(&score(&b, &m)[0]), [500, 625]);
    }

    #[test]
    fn a_lone_note_does_not_make_a_tuplet() {
        let b = bars(1, 1, &[]);
        let mut m = matcher(&b);
        play(&mut m, 0.17, 0.3, 40); // A third of the beat in.
        let s = score(&b, &m);
        assert!(s[0].events.iter().all(|e| e.tuplet.is_none()));
    }

    #[test]
    fn a_slightly_early_note_belongs_to_the_next_beat() {
        let b = bars(1, 1, &[]);
        let mut m = matcher(&b);
        play(&mut m, 0.49, 0.9, 40);
        assert_eq!(note_times(&score(&b, &m)[0]), [500]);
    }

    #[test]
    fn extra_note_moves_off_an_occupied_position() {
        let b = bars(1, 2, &[(1, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.25, 0.4, 40); // Matched to the off-beat.
        // Extra, as its target is taken, rounding onto the same position.
        let extra = m.on_onset(0.27, -10.0);
        m.on_offset(extra, 0.4, -10.0, Some(midi_to_freq(40.0, A4)));
        assert_eq!(m.played[extra].matched, None);
        assert_eq!(note_times(&score(&b, &m)[0]), [250, 375]); // Late, so to the right.
    }

    #[test]
    fn empty_bar_is_a_bar_rest() {
        let b = bars(2, 1, &[]);
        let m = matcher(&b);
        let s = score(&b, &m);
        assert!(matches!(
            s[1].events[..],
            [ScoreEvent {
                kind: ScoreKind::BarRest,
                ..
            }]
        ));
    }

    #[test]
    fn rests_stop_at_the_current_time() {
        let b = bars(2, 1, &[]);
        let m = matcher(&b);
        let s = build(&ScoreInput {
            bars: &b,
            matcher: &m,
            chords: &[],
            scale: None,
            a4: A4,
            until: 1.0,
        });
        assert_eq!(s[0].events.len(), 1);
        assert!(s[1].events.is_empty());
    }

    #[test]
    fn a_sounding_note_appears_live_once_its_pitch_is_known() {
        let b = bars(1, 1, &[(0, 1)]);
        let mut m = matcher(&b);
        let i = m.on_onset(0.0, -10.0);
        let live = |m: &Matcher| {
            build(&ScoreInput {
                bars: &b,
                matcher: m,
                chords: &[],
                scale: None,
                a4: A4,
                until: 0.1,
            })
        };
        assert!(!matches!(live(&m)[0].events[0].kind, ScoreKind::Note(_)));
        m.played[i].freq = Some(82.41);
        assert!(matches!(live(&m)[0].events[0].kind, ScoreKind::Note(_)));
        // A finished note without a pitch still shows, with a neutral head.
        m.played[i].freq = None;
        m.played[i].offset = Some(0.4);
        assert_eq!(note(&live(&m)[0].events[0]).pitch, None);
    }

    #[test]
    fn values_dots_and_ties_within_a_bar() {
        // A dotted quarter, an eighth and a half note.
        let b = bars(1, 2, &[(0, 3), (3, 1), (4, 4)]);
        let mut m = matcher(&b);
        play(&mut m, 0.0, 0.7, 40);
        play(&mut m, 0.75, 0.99, 40);
        play(&mut m, 1.0, 1.99, 40);
        let s = score(&b, &m);
        assert_eq!(
            values(&s[0]),
            [(0, 4, 1, true), (750, 8, 0, true), (1000, 2, 0, true)]
        );
        // Five steps need a tie.
        let parts = binary(5, 8);
        assert_eq!(
            parts,
            [
                (
                    0,
                    NoteValue {
                        denominator: 2,
                        dots: 0
                    }
                ),
                (
                    4,
                    NoteValue {
                        denominator: 8,
                        dots: 0
                    }
                ),
            ]
        );
    }

    #[test]
    fn notes_tie_across_the_barline() {
        let b = bars(2, 1, &[]);
        let mut b = b;
        b[0].notes = vec![crate::plan::PlannedNote {
            time: 1.5,
            len: 1.0,
            accent: false,
        }];
        let mut m = matcher(&b);
        play(&mut m, 1.5, 2.5, 30); // F#, with an accidental.
        let s = score(&b, &m);
        let first = note(s[0].events.last().unwrap());
        assert!(first.tied && !first.continuation);
        assert_eq!(first.pitch.unwrap().accidental, Some(Accidental::Sharp));
        let second = note(&s[1].events[0]);
        assert!(!second.tied && second.continuation);
    }

    #[test]
    fn triplets_keep_whole_beats_plain() {
        let b = bars(1, 3, &[(0, 3), (3, 1), (4, 1), (5, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.0, 0.45, 40);
        for i in 3..6 {
            let t = i as f64 * 0.5 / 3.0;
            play(&mut m, t, t + 0.15, 40);
        }
        let s = score(&b, &m);
        let e = &s[0].events;
        assert_eq!((e[0].value.denominator, e[0].tuplet), (4, None));
        for e in &e[1..4] {
            assert_eq!((e.value.denominator, e.tuplet), (8, Some(3)));
        }
        assert_eq!(note(&e[1]).beam_group, note(&e[3]).beam_group);
        assert!(note(&e[1]).beam_group.is_some());
    }

    #[test]
    fn sixths_are_tuplets_of_sixteenths() {
        let b = bars(1, 6, &[(0, 1), (1, 2), (3, 1), (4, 1), (5, 1)]);
        let mut m = matcher(&b);
        let step = 0.5 / 6.0;
        for s in [0, 1, 3, 4, 5] {
            let t = s as f64 * step;
            let steps = if s == 1 { 2.0 } else { 1.0 };
            play(&mut m, t, t + step * steps * 0.9, 40);
        }
        let s = score(&b, &m);
        let e = &s[0].events;
        assert_eq!((e[0].value.denominator, e[0].tuplet), (16, Some(6)));
        assert_eq!((e[1].value.denominator, e[1].tuplet), (8, Some(6)));
    }

    #[test]
    fn beams_stay_within_a_beat() {
        let b = bars(1, 2, &[(1, 1), (2, 1), (3, 1)]);
        let mut m = matcher(&b);
        for s in 1..4 {
            let t = s as f64 * 0.25;
            play(&mut m, t, t + 0.2, 40);
        }
        let s = score(&b, &m);
        let groups: Vec<_> = s[0]
            .events
            .iter()
            .filter(|e| matches!(e.kind, ScoreKind::Note(_)))
            .map(|e| note(e).beam_group)
            .collect();
        assert_eq!(groups, [None, Some(0), Some(0)]);
    }

    #[test]
    fn unknown_pitch_clash_and_chord_marks() {
        let b = bars(1, 1, &[(0, 1), (1, 1), (2, 1)]);
        let mut m = matcher(&b);
        let chords = [ScoreChord {
            start: 0.0,
            end: 2.0,
            tones: vec![4, 7, 11], // E minor.
            label: "Em".into(),
        }];
        let known = play(&mut m, 0.0, 0.4, 28);
        m.on_pitch(known, Some(1.0));
        let wrong = play(&mut m, 0.5, 0.9, 29);
        m.on_pitch(wrong, Some(0.0));
        let unknown = m.on_onset(1.0, -10.0);
        m.on_offset(unknown, 1.4, -10.0, None);
        let s = build(&ScoreInput {
            bars: &b,
            matcher: &m,
            chords: &chords,
            scale: None,
            a4: A4,
            until: f64::INFINITY,
        });
        let e = &s[0].events;
        assert!(!note(&e[0]).clash);
        assert_eq!(note(&e[0]).chord_marks, [-2, 0, 2]); // E, G, B.
        assert!(note(&e[1]).clash);
        assert_eq!(note(&e[1]).chord_marks, [-2, 0, 2]);
        assert_eq!(note(&e[2]).pitch, None);
        assert!(note(&e[2]).chord_marks.is_empty());
    }

    #[test]
    fn early_note_fits_the_chord_it_is_written_under() {
        let b = bars(2, 1, &[]);
        let mut m = matcher(&b);
        let chords = [
            ScoreChord {
                start: 0.0,
                end: 2.0,
                tones: vec![4, 7, 11], // E minor.
                label: "Em".into(),
            },
            ScoreChord {
                start: 2.0,
                end: 4.0,
                tones: vec![5, 9, 0], // F major.
                label: "F".into(),
            },
        ];
        // F, played 40 ms before the change to F, so judged a clash under Em.
        let early = play(&mut m, 1.96, 2.4, 29);
        m.on_pitch(early, Some(0.0));
        let s = build(&ScoreInput {
            bars: &b,
            matcher: &m,
            chords: &chords,
            scale: None,
            a4: A4,
            until: f64::INFINITY,
        });
        let e = s[1]
            .events
            .iter()
            .find(|e| matches!(e.kind, ScoreKind::Note(_)));
        let n = note(e.expect("written in the second bar"));
        assert!(!n.clash);
        assert_eq!(n.chord_marks.len(), 3);
        assert_eq!(n.chord_marks[0], staff_pitch(29, false).pos); // From F, not E.
    }

    #[test]
    fn chord_marks_stack_from_the_root_below() {
        // Note G1 over C major: root C1 is below it.
        assert_eq!(
            chord_marks(31, &[0, 4, 7], &Spelling::chromatic(false)),
            [-4, -2, 0]
        );
    }

    #[test]
    fn bpm_shows_on_the_first_bar_and_changes() {
        let mut b = bars(3, 1, &[]);
        b[2].bpm = 125.0;
        let s = score(&b, &matcher(&b));
        let shown: Vec<bool> = s.iter().map(|b| b.show_bpm).collect();
        assert_eq!(shown, [true, false, true]);
    }

    #[test]
    fn count_in_is_left_out() {
        let mut b = bars(2, 1, &[]);
        b[0].index = -1;
        assert_eq!(score(&b, &matcher(&b)).len(), 1);
    }

    #[test]
    fn stale_match_is_treated_as_an_extra_note() {
        let b = bars(1, 1, &[(0, 1), (1, 1)]);
        let mut m = matcher(&b);
        let i = play(&mut m, 0.45, 0.6, 40);
        m.expected.truncate(1);
        m.outcomes.truncate(1);
        assert_eq!(m.played[i].matched, Some(1));
        assert_eq!(note_times(&score(&b, &m)[0]), [500]);
    }

    #[test]
    fn wrapping_into_rows() {
        assert_eq!(rows(5, 2), [0..2, 2..4, 4..5]);
        assert!(rows(0, 2).is_empty());
        let b = bars(1, 1, &[]);
        let s = score(&b, &matcher(&b));
        assert_eq!(bars_per_row(&s, 1000.0, 8.0), 4);
        assert_eq!(bars_per_row(&s, 1000.0, 16.0), 2);
        assert_eq!(bars_per_row(&s, 10.0, 8.0), 1);
    }

    #[test]
    fn extent_includes_ledger_notes_and_marks() {
        let b = bars(1, 1, &[(0, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.0, 0.4, 23);
        assert_eq!(extent(&score(&b, &m)), (-5, TOP_LINE));
    }
}
