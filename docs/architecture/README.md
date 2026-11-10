# Architecture notes

Working notes on how the code fits together, kept so nobody has to re-read the whole
codebase to get started. Read these first, then open only the files you need.

References use `file.rs::symbol` rather than line numbers, so they survive edits. If a
doc disagrees with the code, the code wins: fix the doc in the same commit.

| Doc | Covers | Last verified at |
|---|---|---|
| [audio.md](audio.md) | Threads, rings, capture, mix, clock, latency, session lifecycle, scheduling, note detection | `e0091e1` |
| [ui.md](ui.md) | App shell, widget conventions, theme, beatgrid, results dialog, settings, presets | `e0091e1` |
| [ui-components.md](ui-components.md) | Component kit: tiers, Props / State / Msg / Emission, file layout, themes, testing, migration status | `(new)` |
| [data-model.md](data-model.md) | Plans, scoring, chords, what survives a session, time bases | `e0091e1` |

Update the "last verified" commit when you re-check a doc against the code, not on
every edit.

## Crates

Dependencies point one way:
`bg-core` / `bg-dsp` -> `bg-sound` -> `bg-audio` -> `bg-report` -> `bg-ui` -> `bg-exercises`
-> `bass-gym`.

| Crate | Role | Depends on |
|---|---|---|
| `bg-core` | Music time, grooves, `SessionConfig`, `Planner`, `Matcher`, harmony, presets. No audio. | none |
| `bg-dsp` | Pitch tracker, `detect()`, `NoteDetector`, biquads | serde only |
| `bg-sound` | `Voice` / `SoundSource` registry, synth voices, SF2 through `rustysynth` | bg-core |
| `bg-audio` | cpal streams, `Clock`, output scheduler, analysis worker, `SessionLogic`, calibration | bg-core, bg-dsp, bg-sound, cpal, rtrb |
| `bg-report` | Bug reports: WAV, JSON, MIDI and zip of a recording's region | bg-core, bg-audio, bg-sound |
| `bg-ui` | Themes, `ExerciseType` / `PreferencePage` traits, `SettingsStore`, widgets | bg-core, bg-audio, bg-report |
| `bg-exercises` | Basic timing, Basic chords, `TimingSession`, `Accompaniment` | bg-core, bg-audio, bg-sound, bg-ui |
| `bass-gym` | App shell, preference pages, directories | all of the above |
| `sf2pack` | FLAC to SF2 packing tool | standalone (dev-dep of bg-sound) |

What this means in practice:
- Widgets in `bg-ui` can name `Chord`, `Scale`, `Matcher` and `BarPlan` (bg-core), but
  not `ChordSpan`, `ChordShared` or `PianoNote` (bg-exercises).
- `bg-audio` can't name exercise types either. `Slot` is in `bg-audio`, `SoundEvent` is
  in `bg-sound`.

## Commands

- Run: `cargo run -p bass-gym --release`
- Test: `cargo test --workspace`
- Format: `cargo fmt` (edition 2024 `rustfmt.toml`; a pre-commit hook lives in
  `.githooks/`)
- Build soundfonts: `cargo soundfonts` (`assets/soundfonts/*.sf2` is git-ignored)

## Debug environment variables

Handled in `bass-gym/src/app.rs` and `main.rs`:

| Variable | Effect |
|---|---|
| `BASS_GYM_SCREENSHOT=<file.ppm>` | Saves the window after 2 s and quits |
| `BASS_GYM_SIZE=<w>x<h>` | Fixed window size |
| `BASS_GYM_EXERCISE=<id>` | Exercise shown (`basic_timing`, `diatonic_chords`) |
| `BASS_GYM_PREFS=<page id>` | Opens a preferences page (drawn in the main window in screenshot mode) |
| `BASS_GYM_FAKE_SESSION=1` | Runs `bg-exercises/src/fake_session.rs` and opens the Score + Audio tab; `=editor` opens the marker editor too, `=grid` leaves the results closed to show the beat grid |

For screenshots, set `HOME` to a scratch folder so the real settings and presets stay
untouched: the fake session changes the exercise's session settings.

## Conventions

- `//!` module docs, short comments, British spelling ("colour").
- Additive config changes need only `#[serde(default)]`. Preset restructures need a
  `current_version` bump plus `migrate_step`.
- Keep logic in pure functions outside painters, because that's what's easiest to test.
  Interaction can be tested too, with `kit::Rig` (see `ui-components.md`).
- Tests: `#[cfg(test)]` modules, plus offline end-to-end tests in
  `crates/bg-exercises/tests/` (`pipeline.rs`, `basic_chords.rs`). These synthesise a
  `pluck`, run it through `NoteDetector` and `TimingSession` using `SessionIo::offline`,
  and assert on `Matcher`. They re-implement the worker loop; `worker.rs` and
  `clock.rs` have no tests of their own.
- `samples/e-piano/` holds LFS FLACs for `sf2pack`. Nothing else reads `samples/`.
- This is a solo repo: commit straight to main. Stage files by name, because large
  untracked sample folders may be sitting in the tree.
