# Data model

## Core types

```rust
// bg-core/src/plan.rs
pub struct BarPlan {
    pub index: i64,  // negative = count-in
    pub start: f64, pub duration: f64, pub bpm: f32, pub beats: u32,
    pub sig: TimeSignature, pub steps_per_beat: u32, pub muted: bool,
    pub clicks: Vec<PlannedClick>, pub notes: Vec<PlannedNote>,
}
// PlannedNote is { time, len, accent }, in seconds.

// bg-core/src/scoring.rs
pub enum Grade { Good, Ok, Poor, Missed }           // ordered; is_hit = Good | Ok
pub enum GradeMode { Combined, Separate }
pub struct Expected {
    pub bar: i64, pub bpm: f32, pub time: f64, pub len: f64,
    pub accent: bool, pub window: f64,
}
pub struct Played {
    pub onset: f64, pub offset: Option<f64>, pub peak_db: f32,
    pub freq: Option<f32>, pub harmony: Option<f32>,
    pub matched: Option<usize>,   // None = extra note
}
pub struct Outcome {
    pub played: Option<usize>, pub timing_ms: Option<f32>,
    pub length_ratio: Option<f32>, pub harmony: Option<f32>,
    pub harmony_grade: Option<Grade>, pub grade: Option<Grade>,
}
pub struct BarInfo { pub index: i64, pub start: f64, pub duration: f64, pub bpm: f32 }
pub struct Matcher {
    pub thresholds: ScoringThresholds, pub bars: Vec<BarInfo>,
    pub harmony_enabled: bool, pub grade_mode: GradeMode,
    pub expected: Vec<Expected>, pub outcomes: Vec<Outcome>, pub played: Vec<Played>,
    /* private: first_open, now */
}

// bg-exercises/src/timing_session.rs
pub struct TimingShared { pub bars: Vec<BarPlan>, pub matcher: Matcher, pub running: bool, pub bpm: f32 }

// bg-exercises/src/basic_chords/accompaniment.rs
pub struct ChordSpan { pub start: f64, pub end: f64, pub step: usize, pub chord: Chord, pub voicing: Vec<u8> }
pub struct PianoNote { pub time: f64, pub len: f64, pub midi: u8 }
pub struct ChordShared { pub progression: Vec<Chord>, pub spans: Vec<ChordSpan>, pub piano: Vec<PianoNote> }

// bg-core/src/harmony/mod.rs
pub struct Scale { pub root: u8, pub mode: Mode }
pub struct Chord { pub degree: usize, pub triad: [u8; 3],
                   pub seventh: Option<u8>, pub colours: Vec<(u8, u8)> }
pub trait ChordChecker: Sync {
    fn id(&self) -> &'static str; fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn score(&self, pc: u8, chord: &Chord, scale: &Scale) -> f32;
}
```

`Chord` has `tones()` (root first), `contains()`, `is_chord_tone()`, `interval()`,
`name()` and `roman()`. `Scale` has `pitch_classes()`, `contains()`, `chord()` and
`realize()`. The checkers are `RootOnly`, `ChordTones` and `ChordOrScale`.

`SessionConfig` and `Scale` derive `Serialize`. `BarPlan`, `ClickKind`, `Expected`,
`Played`, `Outcome`, `Chord`, `ChordSpan`, `PianoNote` and `SoundEvent` don't, and
`bg-sound` has no serde dependency.

## Time bases: don't mix these

| Value | Base |
|---|---|
| `BarPlan`, `Expected`, `Played`, `ChordSpan`, `PianoNote` | Absolute transport seconds |
| `BarStats.start` | Relative to the session start |
| `NoteEvent.sample` | Input samples since `Analysis` was created |
| `BarNote.pos` / `len` (`groove.rs`) | Ticks, `PPQ = 960` |

Transport time resets when the output stream is rebuilt.

## Planning

- `Planner::next_bar` turns a groove into `BarPlan`s. Ticks are dropped at that point:
  `PlannedNote` keeps only seconds. They can be recovered from `bar.start` and
  `bar.duration / beats`, since tempo only changes at bar starts.
- `TempoRamp { enabled, target, step, every }` steps the tempo at bar boundaries only,
  decided when the bar is planned (up to ~2 bars ahead). There are no continuous ramps.
- `Trigger::Success` depends on performance, and `new_seed_each_start` changes the seed.
  To replay a session, use logged data, not a re-run of `Planner`.
- `bpm` refers to the signature's beat unit. Each `BarPlan` carries its `sig` and
  `steps_per_beat` (from `SessionConfig.time_sig` and `.groove`).
- The UI shows bar `index + 1`.

## Grading (`bg-core/src/scoring.rs`)

- `regrade` takes the worst of timing, length and (in Combined mode) harmony.
- Default thresholds: good 12 ms, ok 30 ms, window 150 ms, length 0.3 / 0.6.
- Each note's window is clamped to half the distance to its neighbours.
- `Missed` is set in `update` once the window passes with nothing played.
- Extra notes have no grade. The beatgrid draws them red.

## Pitch and harmony

- **There are no expected pitches.** `Expected` is rhythm only, so a wrong note can only
  mean a clash with the current chord, which exists only in Basic chords.
- `Played.freq` is the only pitch data. It's stored as soon as early pitch detection
  reports, with or without an accompaniment. Convert with `music.rs::freq_to_midi(freq, a4)`.
  `a4` comes from `TunerPrefs`. `freq` can stay `None`.
- `harmony` is the checker's score: 1 chord tone, 0.5 passing, 0 clash. `harmony_grade`
  maps it to Good, Ok or Poor.
- The degree played isn't stored. Derive it from the chord at the target time when
  matched, otherwise at the onset (as `accompaniment.rs::judge` does).
- `music.rs::note_name` is sharps only. `PPQ = 960`.

## What survives a session

Everything lives in memory and is replaced by `SessionLogic::start`. It's hidden when
the config changes (`has_results = false`).

- `shared.bars` keeps all bars started before the stop, count-in included. Clicks after
  the stop moment stay listed in them.
- `matcher.bars` excludes count-in and isn't trimmed at stop.
- `expected` and `outcomes` are truncated at stop. `played` is kept whole, minus
  count-in notes, which are dropped on arrival.
- `ChordShared.spans` and `piano` are trimmed at stop, but notes aren't shortened.
  `PianoNote` has no velocity or gain.
- Groove-click notes aren't logged. Which sounds were used and the master gain live in
  `SoundConfig`, not per session.

## Pitfalls

- `Matcher::truncate_expected` clears `played[].matched` values past the cut, but still
  prefer `.get(i)` on matcher indices.
- The worker takes the timing lock and the chords lock separately (`TimingSession::judge`
  releases one before the other). Never hold both: take chord data first and release it.
- `shared.running = false` happens inside `stop`, before the accompaniment trims chords.
  Wait for `!engine.session_running()` before reading final session data.
