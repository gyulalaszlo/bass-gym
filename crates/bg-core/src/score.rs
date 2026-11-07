//! A bass-clef score of what was played: turns matched and extra notes into bars of
//! notes, rests and ties for display. Pure layout data, shared by the results view and
//! the live score above the beat grid.
//!
//! Notes sit on the groove's step grid, not at their played time: a matched note takes
//! its target's position and length, an extra note rounds to the nearest step. Bass is
//! written an octave above sounding pitch.

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
}

/// A note's place on the staff: `pos` counts lines and spaces up from the bottom line
/// (0 = bottom line, 1 = first space, 8 = top line; negative = below the staff).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaffPitch {
    pub pos: i32,
    pub accidental: Option<Accidental>,
}

/// Where a sounding MIDI note is written. There's no key signature, so every altered
/// note carries its accidental; `flats` picks the spelling.
pub fn staff_pitch(sounding_midi: i32, flats: bool) -> StaffPitch {
    let written = sounding_midi + 12;
    let octave = written.div_euclid(12) - 1;
    let table = if flats { &FLAT_LETTERS } else { &SHARP_LETTERS };
    let (letter, altered) = table[written.rem_euclid(12) as usize];
    let accidental = altered.then_some(if flats {
        Accidental::Flat
    } else {
        Accidental::Sharp
    });
    StaffPitch {
        pos: octave * 7 + letter - BOTTOM_LINE,
        accidental,
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
    /// Step within the bar.
    pub step: u32,
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

/// A played note on the grid, in steps counted from the first bar.
struct GridNote {
    start: u32,
    len: u32,
    played: usize,
}

/// Builds the score. Count-in bars are left out. `bars` may be any contiguous part of
/// the session; notes outside it are skipped.
pub fn build(input: &ScoreInput) -> Vec<ScoreBar> {
    let bars: Vec<&BarPlan> = input.bars.iter().filter(|b| !b.is_count_in()).collect();
    let mut first_step = Vec::with_capacity(bars.len() + 1);
    let mut total = 0;
    for b in &bars {
        first_step.push(total);
        total += bar_steps(b);
    }
    first_step.push(total);

    let notes = grid_notes(input, &bars, &first_step);
    let flats = input.scale.is_some_and(|s| s.prefers_flats());
    let mut out: Vec<ScoreBar> = Vec::with_capacity(bars.len());
    let mut last_bpm = None;
    for (i, bar) in bars.iter().enumerate() {
        let mut events = bar_events(bar, first_step[i], &notes, input, flats);
        beam(&mut events, bar.steps_per_beat.max(1));
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

fn bar_steps(bar: &BarPlan) -> u32 {
    bar.beats.max(1) * bar.steps_per_beat.max(1)
}

fn step_len(bar: &BarPlan) -> f64 {
    bar.duration / bar_steps(bar) as f64
}

/// Places every played note on the grid, one voice: a note ends where the next starts.
fn grid_notes(input: &ScoreInput, bars: &[&BarPlan], first_step: &[u32]) -> Vec<GridNote> {
    let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
        return Vec::new();
    };
    let m = input.matcher;
    let total = *first_step.last().unwrap_or(&0);
    let window = m.thresholds.window_ms as f64 / 1000.0;
    let from = m.played.partition_point(|p| p.onset < first.start - window);
    let to = m.played.partition_point(|p| p.onset < last.end() + window);

    let mut notes: Vec<GridNote> = Vec::new();
    let mut extras = Vec::new();
    for (i, p) in m.played.iter().enumerate().take(to).skip(from) {
        // In a live view a note waits for its pitch, which comes shortly after the onset.
        if input.until.is_finite() && p.freq.is_none() && p.offset.is_none() {
            continue;
        }
        match p.matched.and_then(|e| m.expected.get(e)) {
            Some(e) => {
                let Some(b) = bars.iter().position(|b| b.index == e.bar) else {
                    continue;
                };
                let s = step_len(bars[b]);
                let start = first_step[b] + ((e.time - bars[b].start) / s).round() as u32;
                let len = ((e.len / s).round() as u32).max(1);
                notes.push(GridNote {
                    start,
                    len,
                    played: i,
                });
            }
            None => extras.push(i),
        }
    }
    notes.sort_by_key(|n| n.start);

    for i in extras {
        let p = &m.played[i];
        let b = bars
            .iter()
            .rposition(|b| b.start <= p.onset)
            .unwrap_or_default();
        let s = step_len(bars[b]);
        let exact = (p.onset - bars[b].start) / s;
        let ideal = (first_step[b] as i64 + exact.round() as i64).clamp(0, total as i64 - 1);
        let start = free_step(&notes, ideal, exact.round() < exact, total);
        let len = p
            .offset
            .map_or(1, |o| (((o - p.onset) / s).round() as u32).max(1));
        let at = notes.partition_point(|n| n.start <= start);
        notes.insert(
            at,
            GridNote {
                start,
                len,
                played: i,
            },
        );
    }

    for i in 0..notes.len() {
        let next = notes[i + 1..]
            .iter()
            .find(|n| n.start > notes[i].start)
            .map_or(total, |n| n.start);
        notes[i].len = notes[i].len.min(next - notes[i].start).max(1);
    }
    notes
}

/// The step for an extra note ideally at `ideal`: that step if free, otherwise the
/// neighbour on the side it was played on, then the other one. With none free it stays
/// put and overlaps.
fn free_step(notes: &[GridNote], ideal: i64, played_late: bool, total: u32) -> u32 {
    let free =
        |s: i64| (0..total as i64).contains(&s) && !notes.iter().any(|n| n.start as i64 == s);
    let side = if played_late { 1 } else { -1 };
    [ideal, ideal + side, ideal - side]
        .into_iter()
        .find(|&s| free(s))
        .unwrap_or(ideal) as u32
}

/// The notes and rests of one bar, including parts of notes tied in from earlier bars.
fn bar_events(
    bar: &BarPlan,
    first: u32,
    notes: &[GridNote],
    input: &ScoreInput,
    flats: bool,
) -> Vec<ScoreEvent> {
    let steps = bar_steps(bar);
    let last = first + steps;
    let s = step_len(bar);
    let time = |step: u32| bar.start + step as f64 * s;
    let mut events = Vec::new();
    let mut cursor = 0;
    let rest = |events: &mut Vec<ScoreEvent>, from: u32, to: u32| {
        for (step, value, tuplet) in split(bar, from, to - from) {
            if time(step) < input.until {
                events.push(ScoreEvent {
                    time: time(step),
                    step,
                    value,
                    tuplet,
                    kind: ScoreKind::Rest,
                });
            }
        }
    };
    for n in notes
        .iter()
        .filter(|n| n.start < last && n.start + n.len > first)
    {
        let from = n.start.max(first) - first;
        let to = (n.start + n.len).min(last) - first;
        if from > cursor {
            rest(&mut events, cursor, from);
        }
        let continues = n.start + n.len > last;
        let parts = split(bar, from, to - from);
        let count = parts.len();
        let note = score_note(input, n.played, flats);
        for (k, (step, value, tuplet)) in parts.into_iter().enumerate() {
            let continuation = k > 0 || n.start < first;
            events.push(ScoreEvent {
                time: time(step),
                step,
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
                step: 0,
                value: NoteValue {
                    denominator: 1,
                    dots: 0,
                },
                tuplet: None,
                kind: ScoreKind::BarRest,
            });
        }
    } else if cursor < steps {
        rest(&mut events, cursor, steps);
    }
    events
}

fn score_note(input: &ScoreInput, played: usize, flats: bool) -> ScoreNote {
    let m = input.matcher;
    let p: &Played = &m.played[played];
    let midi = p.freq.map(|f| freq_to_midi(f, input.a4).0);
    let pitch = midi.map(|n| staff_pitch(n, flats));
    let at = p
        .matched
        .and_then(|e| m.expected.get(e))
        .map_or(p.onset, |e| e.time);
    let chord = input
        .chords
        .iter()
        .find(|c| c.start - 1e-6 <= at && at < c.end);
    let chord_marks = match (midi, chord) {
        (Some(n), Some(c)) => chord_marks(n, &c.tones, flats),
        _ => Vec::new(),
    };
    ScoreNote {
        played,
        pitch,
        clash: p.harmony.map(harmony_grade) == Some(Grade::Poor),
        chord_marks,
        tied: false,
        continuation: false,
        stem_up: pitch.map_or(MIDDLE_LINE, |p| p.pos) < MIDDLE_LINE,
        beam_group: None,
    }
}

/// Staff positions of `tones` (root first) stacked within an octave, from the nearest
/// root at or below the sounding note `midi`.
pub fn chord_marks(midi: i32, tones: &[u8], flats: bool) -> Vec<i32> {
    let Some(&root) = tones.first() else {
        return Vec::new();
    };
    let root = root as i32;
    let base = midi - (midi - root).rem_euclid(12);
    tones
        .iter()
        .map(|&t| staff_pitch(base + (t as i32 - root).rem_euclid(12), flats).pos)
        .collect()
}

/// Splits `len` steps from `start` into written values: `(step, value, tuplet)`.
/// On a tuplet grid, whole beats are written plainly and only parts of a beat as tuplets.
fn split(bar: &BarPlan, start: u32, len: u32) -> Vec<(u32, NoteValue, Option<u32>)> {
    let spb = bar.steps_per_beat.max(1);
    let den = bar.sig.den.max(1);
    let mut out = Vec::new();
    if spb.is_power_of_two() {
        for (at, value) in binary(len, den * spb) {
            out.push((start + at, value, None));
        }
        return out;
    }
    let base = 1 << spb.ilog2();
    let (mut pos, end) = (start, start + len);
    while pos < end {
        if pos.is_multiple_of(spb) && end - pos >= spb {
            let beats = (end - pos) / spb;
            for (at, value) in binary(beats, den) {
                out.push((pos + at * spb, value, None));
            }
            pos += beats * spb;
        } else {
            let part = (spb - pos % spb).min(end - pos);
            for (at, value) in binary(part, den * base) {
                out.push((pos + at, value, Some(spb)));
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
fn beam(events: &mut [ScoreEvent], spb: u32) {
    let mut group = 0;
    let mut i = 0;
    while i < events.len() {
        let beamable = |e: &ScoreEvent| matches!(e.kind, ScoreKind::Note(_)) && e.value.beams() > 0;
        let mut end = i;
        while end < events.len()
            && beamable(&events[end])
            && events[end].step / spb == events[i].step / spb
        {
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

/// Bars per row for a row `width` wide, keeping the densest bar readable.
pub fn bars_per_row(bars: &[ScoreBar], width: f32) -> usize {
    let densest = bars.iter().map(|b| b.events.len()).max().unwrap_or(0);
    let bar_width = (densest as f32 * 40.0 + 40.0).max(220.0);
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

    fn values(bar: &ScoreBar) -> Vec<(u32, u32, u8, bool)> {
        bar.events
            .iter()
            .map(|e| {
                let note = matches!(e.kind, ScoreKind::Note(_));
                (e.step, e.value.denominator, e.value.dots, note)
            })
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

    #[test]
    fn matched_notes_take_the_target_grid_position() {
        let b = bars(1, 1, &[(0, 1), (1, 1), (2, 1), (3, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.02, 0.3, 40); // Late and short.
        play(&mut m, 0.48, 0.9, 40); // Early.
        let s = score(&b, &m);
        assert_eq!(
            values(&s[0]),
            [
                (0, 4, 0, true),
                (1, 4, 0, true),
                (2, 2, 0, false), // The two unplayed beats.
            ]
        );
        assert!((s[0].events[1].time - 0.5).abs() < 1e-9);
    }

    #[test]
    fn extra_notes_round_to_the_nearest_step() {
        let b = bars(1, 2, &[(0, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.76, 0.99, 40); // Nearest eighth step is 3 (0.75 s).
        let s = score(&b, &m);
        let steps: Vec<u32> = s[0]
            .events
            .iter()
            .filter(|e| matches!(e.kind, ScoreKind::Note(_)))
            .map(|e| e.step)
            .collect();
        assert_eq!(steps, [3]);
    }

    #[test]
    fn extra_note_moves_off_an_occupied_step() {
        let b = bars(1, 2, &[(1, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.25, 0.4, 40); // Matched to step 1.
        // Extra, outside the window of an already played target, rounding onto step 1.
        let extra = m.on_onset(0.27, -10.0);
        m.on_offset(extra, 0.4, -10.0, Some(midi_to_freq(40.0, A4)));
        assert_eq!(m.played[extra].matched, None);
        let s = score(&b, &m);
        let steps: Vec<u32> = s[0]
            .events
            .iter()
            .filter(|e| matches!(e.kind, ScoreKind::Note(_)))
            .map(|e| e.step)
            .collect();
        assert_eq!(steps, [1, 2]); // Played late, so it moves right.
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
            [(0, 4, 1, true), (3, 8, 0, true), (4, 2, 0, true)]
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
        let b = bars(1, 6, &[]);
        let parts = split(&b[0], 1, 2);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].1.denominator, 8);
        assert_eq!(parts[0].2, Some(6));
        let one = split(&b[0], 1, 1);
        assert_eq!(one[0].1.denominator, 16);
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
    fn chord_marks_stack_from_the_root_below() {
        // Note G1 over C major: root C1 is below it.
        assert_eq!(chord_marks(31, &[0, 4, 7], false), [-4, -2, 0]);
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
        assert_eq!(score(&b, &m)[0].events.len(), 3);
    }

    #[test]
    fn wrapping_into_rows() {
        assert_eq!(rows(5, 2), [0..2, 2..4, 4..5]);
        assert!(rows(0, 2).is_empty());
        let b = bars(1, 1, &[]);
        let s = score(&b, &matcher(&b));
        assert_eq!(bars_per_row(&s, 1000.0), 4);
        assert_eq!(bars_per_row(&s, 10.0), 1);
    }

    #[test]
    fn extent_includes_ledger_notes_and_marks() {
        let b = bars(1, 1, &[(0, 1)]);
        let mut m = matcher(&b);
        play(&mut m, 0.0, 0.4, 23);
        assert_eq!(extent(&score(&b, &m)), (-5, TOP_LINE));
    }
}
