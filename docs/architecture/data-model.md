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
    pub noise: Vec<Noise>,   // string noise { onset, offset, peak_db }: review only
    /* private: first_open, now */
}

// bg-exercises/src/timing_session.rs
pub struct TimingShared { pub bars: Vec<BarPlan>, pub matcher: Matcher, pub running: bool, pub bpm: f32 }

// bg-core/src/session.rs: SessionConfig { tempo, time_sig, key: Scale, groove, .. }
// `key` is every exercise's key (set in the toolbar): Basic chords' scale and the score's
// key signature.

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

## Targets

`Expected` notes come from `TimingSession::tick`, not straight from `BarPlan.notes`.
`merge_targets` combines the groove's notes with the attacks `Accompaniment::plan_bar`
returns: grid times, before humanize, anticipations included. Attacks within a quarter of
a grid step of a groove note merge into it. Basic chords' `ScoringOptions { groove, piano,
chord_tones }` (in the preset) maps to `AccompanimentScoring { groove, hits, harmony }`.
The groove click always plays from `bar.notes`, and `shared.bars` keeps the original bar.
With no targets, every note is an extra, `bar_rate` is `None` and success-triggered ramps
never fire. `timing_session::preview_targets` does the same merge for the preview grid,
using `accompaniment::preview_targets`, which leaves out the random anticipations.

## Grading (`bg-core/src/scoring.rs`)

- `on_onset` takes the nearest target that's free or held by a note further from it, so
  string noise just before a note can't keep the note from its target.
- `regrade` takes the worst of timing, length and (in Combined mode) harmony.
- Default thresholds: good 12 ms, ok 30 ms, window 150 ms, length 0.3 / 0.6. Whether
  length is scored comes from the preset (`SessionConfig.score_length`), which
  `TimingSession::new` copies into `ScoringThresholds.score_length`. The tolerances are
  global.
- Each note's window is clamped to half the distance to its neighbours.
- `Missed` is set in `update` once the window passes with nothing played.
- A note with no pitch at its offset is a ghost note (a muted pluck): `length_ratio`
  stays `None`, so only its timing is graded.
- Extra notes have no grade. The beatgrid draws them red.

## Pitch and harmony

- **There are no expected pitches.** `Expected` is rhythm only, so a wrong note can only
  mean a clash with the current chord, which exists only in Basic chords.
- `Played.freq` is the only pitch data. It's stored as soon as early pitch detection
  reports, with or without an accompaniment. Convert with `music.rs::freq_to_midi(freq, a4)`.
  `a4` comes from `TunerPrefs`. `freq` can stay `None`.
- `harmony` is the checker's score: 1 chord tone, 0.5 passing, 0 clash. `harmony_grade`
  maps it to Good, Ok or Poor.
- The degree played isn't stored. Derive it from the chord at
  `timing_session.rs::judge_time`: the target's time when matched, otherwise 30 ms after
  the onset, so a note played just before a chord change counts for the new chord.
- With `harmony` off (`AccompanimentScoring`), the matcher has `harmony_enabled = false`
  and pitches are stored but never judged.
- `stats().harmony` counts matched notes, plus extras whose pitch was judged. The
  "unknown" bucket only counts matched notes.
- `music.rs::note_name` is sharps only. `PPQ = 960`.

## Score (`bg-core/src/score.rs`)

`score::build(&ScoreInput)` turns bars and played notes into `ScoreBar`s of notes and
rests. It's pure, so all notation rules are tested there and painters stay simple.

- Noteheads sit on a musical grid, not at the played time and not on the exercise's
  grid. A matched note starts at its target. Every beat picks the simplest of
  `SUBDIVISIONS` (quarter, eighth, sixteenth, triplet, sextuplet) that fits its notes
  within `FIT_TOLERANCE`, and extras round to that. A lone note can't make a tuplet.
- Notes never swap order: an extra is placed between its neighbours in playing order.
  If its position is taken it moves by the smallest step. Pitchless notes are ghost
  notes (x noteheads) and are placed like any other; string noise never reaches the
  score (see audio.md, hold stage).
- Lengths come from the playing, not the target. A note held for `LEGATO` of the way to
  the next one reaches it, so small gaps aren't rests; otherwise it rounds to eighths,
  or a sixteenth if short. While still sounding, a matched note shows its target's length.
- Positions are ticks: `Grid.tpb` per beat, enough for every subdivision.
- One voice: a note ends where the next starts. Ties split notes at barlines and where
  no single value fits. Parts of tuplet beats are written as tuplets, whole beats
  plainly. Rests finish their beat first.
- Bass is written an octave up. `StaffPitch.pos` counts lines and spaces from the bottom
  line (written G2): open E is -2, open G 7.
- Key signature from `ScoreInput.scale` (`KeySignature::of`, via the parent major).
  `Spelling` gives the key's own notes its letters (C flat in G flat major) and the
  rest sharps or flats as the signature is; sharps when there's no scale.
  `apply_key` writes a sign only where a note differs from the signature or an earlier
  sign on that line or space in the bar (naturals included); tied continuations
  carry none.
- `clash` means the note's harmony score grades Poor and the note isn't a tone of the
  chord where it's written. Without chords nothing clashes.
- `chord_marks` stack the chord where the note is written within an octave from the
  nearest root at or below the note. The written time can differ from the judged one
  (`judge_time`): a note played just before a chord change may be written after it. `ScoreChord` is the neutral chord form, since bg-core can't name `ChordSpan`.
- `until` stops rests at the current time for the live view.

## What survives a session

Everything lives in memory and is replaced by `SessionLogic::start`. It's hidden when
the config changes (`has_results = false`).

- `shared.bars` keeps all bars started before the stop, count-in included. Clicks after
  the stop moment stay listed in them.
- `matcher.bars` excludes count-in and isn't trimmed at stop.
- `expected` and `outcomes` are truncated at stop. `played` is kept whole, minus
  count-in notes, which are dropped on arrival.
- `ChordShared.spans` and `piano` are trimmed at stop, but notes aren't shortened.
  An anticipated chord (pushed an eighth before the bar line) gets its own span from
  the push, which the next bar extends, so the strip, chord band and harmony judging
  change with the piano.
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
