//! Harmony: scales, diatonic chords (with colour tones), progressions, piano voicings,
//! comp rhythms and pluggable chord-compatibility checkers.

pub mod comp;
pub mod generate;
pub mod parse;
pub mod voicing;

use serde::{Deserialize, Serialize};

use crate::music::note_name;

pub use generate::{colourize, random_progression, resolve};
pub use parse::{LIBRARY, ParseError, library, parse_progression};
pub use voicing::{Voicing, VoicingStyle, voice};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Ionian,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Aeolian,
    Locrian,
}

impl Mode {
    pub const ALL: [Mode; 7] = [
        Self::Ionian,
        Self::Dorian,
        Self::Phrygian,
        Self::Lydian,
        Self::Mixolydian,
        Self::Aeolian,
        Self::Locrian,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ionian => "Major (Ionian)",
            Self::Dorian => "Dorian",
            Self::Phrygian => "Phrygian",
            Self::Lydian => "Lydian",
            Self::Mixolydian => "Mixolydian",
            Self::Aeolian => "Minor (Aeolian)",
            Self::Locrian => "Locrian",
        }
    }

    /// A short name for tight spaces: "maj", "dor", ...
    pub fn short(self) -> &'static str {
        match self {
            Self::Ionian => "maj",
            Self::Dorian => "dor",
            Self::Phrygian => "phr",
            Self::Lydian => "lyd",
            Self::Mixolydian => "mix",
            Self::Aeolian => "min",
            Self::Locrian => "loc",
        }
    }

    fn rotation(self) -> usize {
        Self::ALL.iter().position(|&m| m == self).unwrap_or(0)
    }
}

const MAJOR_STEPS: [u8; 7] = [2, 2, 1, 2, 2, 2, 1];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scale {
    /// Pitch class of the tonic, 0 = C.
    pub root: u8,
    pub mode: Mode,
}

impl Default for Scale {
    fn default() -> Self {
        Self {
            root: 4,
            mode: Mode::Ionian,
        }
    }
}

impl Scale {
    pub fn pitch_classes(&self) -> [u8; 7] {
        let mut out = [0u8; 7];
        let mut pc = self.root % 12;
        for (i, o) in out.iter_mut().enumerate() {
            *o = pc;
            pc = (pc + MAJOR_STEPS[(i + self.mode.rotation()) % 7]) % 12;
        }
        out
    }

    pub fn contains(&self, pc: u8) -> bool {
        self.pitch_classes().contains(&(pc % 12))
    }

    pub fn name(&self) -> String {
        format!("{} {}", note_name(self.root as i32), self.mode.label())
    }

    /// Whether this scale's key signature is written with flats (judged by its parent
    /// major key), for spelling notes.
    pub fn prefers_flats(&self) -> bool {
        matches!(self.parent_major(), 1 | 3 | 5 | 6 | 8 | 10)
    }

    /// The tonic's name, spelled as the key signature is ("Bb", not "A#").
    pub fn tonic_name(&self) -> &'static str {
        const FLAT_NAMES: [&str; 12] = [
            "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
        ];
        if self.prefers_flats() {
            FLAT_NAMES[(self.root % 12) as usize]
        } else {
            crate::music::note_name(self.root as i32)
        }
    }

    /// Short name of the key, such as "Bb maj" or "F# min".
    pub fn short_name(&self) -> String {
        format!("{} {}", self.tonic_name(), self.mode.short())
    }

    /// Pitch class of the major key sharing this scale's notes (and key signature).
    pub fn parent_major(&self) -> u8 {
        let above_parent: u8 = MAJOR_STEPS[..self.mode.rotation()].iter().sum();
        (self.root % 12 + 12 - above_parent) % 12
    }

    /// Diatonic triad or seventh chord on `degree` (0-based).
    pub fn chord(&self, degree: usize, sevenths: bool) -> Chord {
        self.realize(ChordSpec {
            degree,
            size: if sevenths { 4 } else { 3 },
        })
    }

    /// Builds the diatonic chord for `spec`, stacking thirds up to `spec.size` notes and
    /// leaving out "avoid" colour tones (b9, b13, and the natural 11 over a major third).
    pub fn realize(&self, spec: ChordSpec) -> Chord {
        let pcs = self.pitch_classes();
        let degree = spec.degree % 7;
        let stack = |i: usize| pcs[(degree + 2 * i) % 7];
        let root = stack(0);
        let interval = |pc: u8| (pc + 12 - root) % 12;
        let triad = [root, stack(1), stack(2)];
        let size = spec.size.clamp(3, 7) as usize;
        let seventh = (size >= 4).then(|| stack(3));
        let major_third = interval(triad[1]) == 4;
        let colours = (4..size)
            .map(|i| (EXTENSIONS[i - 4], stack(i)))
            .filter(|&(ext, pc)| {
                let iv = interval(pc);
                !(iv == 1 || iv == 8 || (ext == 11 && iv == 5 && major_third))
            })
            .collect();
        Chord {
            degree,
            triad,
            seventh,
            colours,
        }
    }
}

const EXTENSIONS: [u8; 3] = [9, 11, 13];

/// A chord of a progression, independent of the key: scale degree + how many stacked
/// thirds (3 = triad, 4 = seventh, 5 = ninth, 6 = eleventh, 7 = thirteenth).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordSpec {
    pub degree: usize,
    pub size: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub degree: usize,
    /// Root, third, fifth (pitch classes).
    pub triad: [u8; 3],
    pub seventh: Option<u8>,
    /// Colour tones: (extension number 9 / 11 / 13, pitch class).
    pub colours: Vec<(u8, u8)>,
}

impl Chord {
    pub fn root(&self) -> u8 {
        self.triad[0]
    }

    pub fn third(&self) -> u8 {
        self.triad[1]
    }

    /// All pitch classes, in stacking order.
    pub fn tones(&self) -> Vec<u8> {
        let mut t = self.triad.to_vec();
        t.extend(self.seventh);
        t.extend(self.colours.iter().map(|c| c.1));
        t
    }

    pub fn contains(&self, pc: u8) -> bool {
        self.tones().contains(&(pc % 12))
    }

    /// Root, third, fifth and seventh only.
    pub fn is_chord_tone(&self, pc: u8) -> bool {
        let pc = pc % 12;
        self.triad.contains(&pc) || self.seventh == Some(pc)
    }

    pub fn interval(&self, pc: u8) -> u8 {
        (pc + 12 - self.root()) % 12
    }

    fn triad_quality(&self) -> &'static str {
        match (self.interval(self.triad[1]), self.interval(self.triad[2])) {
            (4, 7) => "",
            (3, 7) => "m",
            (3, 6) => "dim",
            (4, 8) => "aug",
            _ => "?",
        }
    }

    pub fn quality(&self) -> String {
        let triad = self.triad_quality();
        let base = match (triad, self.seventh.map(|s| self.interval(s))) {
            (t, None) => t.to_owned(),
            ("", Some(11)) => "maj7".into(),
            ("", Some(10)) => "7".into(),
            ("m", Some(10)) => "m7".into(),
            ("m", Some(11)) => "m(maj7)".into(),
            ("dim", Some(10)) => "m7b5".into(),
            ("dim", Some(9)) => "dim7".into(),
            (t, Some(_)) => format!("{t}7"),
        };
        if self.colours.is_empty() {
            return base;
        }
        let colours: Vec<String> = self
            .colours
            .iter()
            .map(|&(ext, pc)| {
                let natural = match ext {
                    9 => 2,
                    11 => 5,
                    _ => 9,
                };
                match self.interval(pc) as i32 - natural {
                    0 => ext.to_string(),
                    1 => format!("#{ext}"),
                    _ => format!("b{ext}"),
                }
            })
            .collect();
        format!("{base}({})", colours.join(","))
    }

    pub fn name(&self) -> String {
        format!("{}{}", note_name(self.root() as i32), self.quality())
    }

    pub fn roman(&self) -> String {
        const R: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];
        let r = R[self.degree % 7];
        let triad = self.triad_quality();
        let base = if triad == "m" || triad == "dim" {
            r.to_lowercase()
        } else {
            r.to_owned()
        };
        let mut s = match (triad, self.seventh.map(|p| self.interval(p))) {
            ("dim", Some(10)) => format!("{base}ø7"),
            ("dim", Some(_)) => format!("{base}°7"),
            ("dim", None) => format!("{base}°"),
            (_, Some(11)) => format!("{base}maj7"),
            (_, Some(_)) => format!("{base}7"),
            (_, None) => base,
        };
        if !self.colours.is_empty() {
            let q = self.quality();
            if let Some(i) = q.find('(') {
                s.push_str(&q[i..]);
            }
        }
        s
    }
}

/// Decides how well a played pitch class fits the current chord.
pub trait ChordChecker: Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// 1.0 = fully compatible, 0.0 = clash.
    fn score(&self, pc: u8, chord: &Chord, scale: &Scale) -> f32;
}

pub struct RootOnly;
pub struct ChordTones;
pub struct ChordOrScale;

impl ChordChecker for RootOnly {
    fn id(&self) -> &'static str {
        "root"
    }
    fn name(&self) -> &'static str {
        "Root only"
    }
    fn description(&self) -> &'static str {
        "Only the chord's root counts."
    }
    fn score(&self, pc: u8, chord: &Chord, _: &Scale) -> f32 {
        if pc % 12 == chord.root() { 1.0 } else { 0.0 }
    }
}

impl ChordChecker for ChordTones {
    fn id(&self) -> &'static str {
        "chord_tones"
    }
    fn name(&self) -> &'static str {
        "Chord tones"
    }
    fn description(&self) -> &'static str {
        "Root, third, fifth or seventh count; colour tones and other notes don't."
    }
    fn score(&self, pc: u8, chord: &Chord, _: &Scale) -> f32 {
        if chord.is_chord_tone(pc) { 1.0 } else { 0.0 }
    }
}

impl ChordChecker for ChordOrScale {
    fn id(&self) -> &'static str {
        "chord_or_scale"
    }
    fn name(&self) -> &'static str {
        "Chord tones + scale (weighted)"
    }
    fn description(&self) -> &'static str {
        "Chord tones score fully, colour tones and other scale notes (passing tones) half, anything else zero."
    }
    fn score(&self, pc: u8, chord: &Chord, scale: &Scale) -> f32 {
        if chord.is_chord_tone(pc) {
            1.0
        } else if scale.contains(pc) {
            0.5
        } else {
            0.0
        }
    }
}

pub fn checkers() -> [&'static dyn ChordChecker; 3] {
    [&ChordTones, &ChordOrScale, &RootOnly]
}

pub fn checker(id: &str) -> &'static dyn ChordChecker {
    checkers()
        .into_iter()
        .find(|c| c.id() == id)
        .unwrap_or(&ChordTones)
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: Scale = Scale {
        root: 0,
        mode: Mode::Ionian,
    };

    #[test]
    fn scales_and_chords() {
        assert_eq!(C.pitch_classes(), [0, 2, 4, 5, 7, 9, 11]);
        let a_minor = Scale {
            root: 9,
            mode: Mode::Aeolian,
        };
        assert_eq!(a_minor.pitch_classes(), [9, 11, 0, 2, 4, 5, 7]);
        let names: Vec<String> = (0..7).map(|d| C.chord(d, false).name()).collect();
        assert_eq!(names, ["C", "Dm", "Em", "F", "G", "Am", "Bdim"]);
        let sevenths: Vec<String> = (0..7).map(|d| C.chord(d, true).name()).collect();
        assert_eq!(
            sevenths,
            ["Cmaj7", "Dm7", "Em7", "Fmaj7", "G7", "Am7", "Bm7b5"]
        );
        let romans: Vec<String> = (0..7).map(|d| C.chord(d, true).roman()).collect();
        assert_eq!(
            romans,
            ["Imaj7", "ii7", "iii7", "IVmaj7", "V7", "vi7", "viiø7"]
        );
        assert_eq!(C.chord(6, false).roman(), "vii°");
    }

    #[test]
    fn extensions_skip_avoid_notes() {
        let name = |d, size| C.realize(ChordSpec { degree: d, size }).name();
        assert_eq!(name(0, 7), "Cmaj7(9,13)"); // natural 11 over a major third is avoided
        assert_eq!(name(1, 7), "Dm7(9,11,13)");
        assert_eq!(name(2, 5), "Em7"); // b9 avoided
        assert_eq!(name(3, 6), "Fmaj7(9,#11)");
        assert_eq!(name(4, 7), "G7(9,13)");
        assert_eq!(name(5, 7), "Am7(9,11)"); // b13 avoided
        assert_eq!(C.realize(ChordSpec { degree: 4, size: 5 }).roman(), "V7(9)");
    }

    #[test]
    fn checkers_score() {
        let g7 = C.realize(ChordSpec { degree: 4, size: 5 });
        assert_eq!(checker("chord_tones").score(5, &g7, &C), 1.0); // F in G7
        assert_eq!(checker("chord_tones").score(9, &g7, &C), 0.0); // A is the 9th: colour
        assert_eq!(checker("chord_or_scale").score(9, &g7, &C), 0.5);
        assert_eq!(checker("chord_or_scale").score(1, &g7, &C), 0.0);
        assert_eq!(checker("root").score(7, &g7, &C), 1.0);
    }
}
