//! Basic chords' preset data, and the pure functions on it.

use bg_core::harmony::{self, Chord};
use bg_core::{BarPlan, Matcher, SessionConfig};
use bg_ui::views::keyboard::KeyLabels;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use super::accompaniment::preview_targets;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressionKind {
    /// A chosen library progression or a typed one.
    #[default]
    Fixed,
    /// A random pick from (a selection of) the library on every start.
    SemiRandom,
    /// A generated in-key progression.
    Random,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixedSource {
    #[default]
    Library,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressionSettings {
    pub kind: ProgressionKind,
    pub source: FixedSource,
    /// Library id for `Fixed` + `Library`.
    pub library: String,
    /// Roman numerals for `Fixed` + `Custom`.
    pub custom: String,
    /// Library ids to pick from in `SemiRandom`; empty = all.
    pub pool: Vec<String>,
    pub random_len: u32,
}

impl Default for ProgressionSettings {
    fn default() -> Self {
        Self {
            kind: ProgressionKind::Fixed,
            source: FixedSource::Library,
            library: "pop".into(),
            custom: "ii7 V7 I7 vi7".into(),
            pool: Vec::new(),
            random_len: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChordLength {
    Half,
    #[default]
    One,
    Two,
    Four,
}

impl ChordLength {
    pub const ALL: [ChordLength; 4] = [Self::Half, Self::One, Self::Two, Self::Four];

    pub fn bars(self) -> f32 {
        match self {
            Self::Half => 0.5,
            Self::One => 1.0,
            Self::Two => 2.0,
            Self::Four => 4.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Half => "½ bar",
            Self::One => "1 bar",
            Self::Two => "2 bars",
            Self::Four => "4 bars",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardOptions {
    /// The keyboard is shown at all.
    pub visible: bool,
    pub keys: u32,
    pub labels: KeyLabels,
    /// Outline the next chord shortly before it starts.
    pub ghost_next: bool,
    /// Hide chord names, to practise reading the keyboard.
    pub hide_chord_names: bool,
    pub show_bass: bool,
}

impl Default for KeyboardOptions {
    fn default() -> Self {
        Self {
            visible: true,
            keys: 49,
            labels: KeyLabels::Octaves,
            ghost_next: true,
            hide_chord_names: false,
            show_bass: true,
        }
    }
}

/// What a session grades: which rhythms are targets, and whether notes must fit the chord.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoringOptions {
    /// The groove's notes are targets.
    pub groove: bool,
    /// The piano's attacks are targets.
    pub piano: bool,
    /// Played notes are graded by how they fit the chord (see `checker`).
    pub chord_tones: bool,
}

impl Default for ScoringOptions {
    fn default() -> Self {
        Self {
            groove: true,
            piano: false,
            chord_tones: true,
        }
    }
}

impl ScoringOptions {
    pub fn has_targets(&self) -> bool {
        self.groove || self.piano
    }

    pub fn summary(&self) -> &'static str {
        match (self.has_targets(), self.chord_tones) {
            (false, false) => "Free play: nothing is graded.",
            (false, true) => "Any rhythm: only how the notes fit the chords is graded.",
            (true, false) => "Rhythm only: any pitch is fine.",
            (true, true) => "Rhythm and chord fit are both graded.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasicChordsData {
    /// The key is `session.key`.
    pub session: SessionConfig,
    pub progression: ProgressionSettings,
    pub chord_length: ChordLength,
    /// Comp pattern id (see `harmony::comp::PATTERNS`).
    pub comp: String,
    /// 0 = plain triads … 1 = rich, rootless voicings.
    pub jazziness: f32,
    /// Voicing width: 0 = one hand, 1 = two hands, 2 = two hands with octave doublings.
    pub width: f32,
    /// Register of the comp, in octaves (4 = around middle C).
    pub center_octave: i8,
    /// 0 = machine-exact … 1 = loose timing and dynamics.
    pub humanize: f32,
    pub piano_volume: f32,
    /// Seconds piano notes ring on after their written end (cut by the next attack).
    pub release: f32,
    /// Chord-compatibility checker id.
    pub checker: String,
    pub scoring: ScoringOptions,
    pub seed: u64,
    pub new_seed_each_start: bool,
    pub keyboard: KeyboardOptions,
}

impl Default for BasicChordsData {
    fn default() -> Self {
        Self {
            session: SessionConfig::default(),
            progression: ProgressionSettings::default(),
            chord_length: ChordLength::One,
            comp: "halves".into(),
            jazziness: 0.2,
            width: 0.0,
            center_octave: harmony::voicing::CENTER_OCTAVE,
            humanize: 0.0,
            piano_volume: 0.7,
            release: 0.0,
            checker: "chord_tones".into(),
            scoring: ScoringOptions::default(),
            seed: 1,
            new_seed_each_start: false,
            keyboard: KeyboardOptions::default(),
        }
    }
}

impl BasicChordsData {
    /// How a chord is named on screen: hidden when practising reading the keyboard.
    pub fn chord_label(&self, c: &Chord) -> String {
        if self.keyboard.hide_chord_names {
            "?".into()
        } else {
            c.name()
        }
    }

    /// The preview grid's bars and targets for these settings.
    pub fn preview(&self) -> (Vec<BarPlan>, Matcher) {
        let (scoring, comp) = (&self.scoring, self.comp.as_str());
        let bars_per_chord = self.chord_length.bars();
        crate::timing_session::preview_targets(&self.session, scoring.groove, |bar| {
            if scoring.piano {
                preview_targets(comp, bar, bars_per_chord)
            } else {
                Vec::new()
            }
        })
    }
}

/// The chords a session with `d` and `seed` will play.
pub fn resolve_progression(d: &BasicChordsData, seed: u64) -> Result<Vec<Chord>, String> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let p = &d.progression;
    let text = match p.kind {
        ProgressionKind::Random => {
            return Ok(harmony::random_progression(
                &d.session.key,
                p.random_len.max(1) as usize,
                d.jazziness,
                &mut rng,
            ));
        }
        ProgressionKind::Fixed => match p.source {
            FixedSource::Library => harmony::library(&p.library).map_or("I", |e| e.2).to_owned(),
            FixedSource::Custom => p.custom.clone(),
        },
        ProgressionKind::SemiRandom => {
            use rand::Rng;
            let pool: Vec<&str> = harmony::LIBRARY
                .iter()
                .filter(|e| p.pool.is_empty() || p.pool.iter().any(|id| id == e.0))
                .map(|e| e.2)
                .collect();
            let pool = if pool.is_empty() {
                vec![harmony::LIBRARY[0].2]
            } else {
                pool
            };
            pool[rng.random_range(0..pool.len())].to_owned()
        }
    };
    let specs = harmony::parse_progression(&text, &d.session.key).map_err(|e| e.to_string())?;
    Ok(harmony::resolve(
        &specs,
        &d.session.key,
        d.jazziness,
        &mut rng,
    ))
}
