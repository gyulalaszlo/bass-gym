# bass-gym

A modular desktop app for practising bass: always-visible high-accuracy tuner, exercise types with versioned presets, and a scrolling, colour-coded beat grid that scores timing, note length and level.

```
cargo run -p bass-gym --release     # app
cargo test --workspace              # unit + offline end-to-end tests
```

Settings, presets and session state are stored in the platform config dir (macOS: `~/Library/Application Support/bass-gym`).

## Crates

| crate | role |
|---|---|
| `bg-core` | music time, grooves, `SessionConfig`, bar `Planner` (count-in, tempo ramps, click gaps, auto stop), `Matcher` scoring (timing, length, harmony), harmony engine (scales, chords, roman-numeral parser, progression library and generator, voicings, comp patterns), versioned presets |
| `bg-dsp` | MPM pitch tracker (tuner, ±1 ¢ from B0), one-shot per-note pitch detection, note onset/offset detector |
| `bg-sound` | `Voice` / `SoundSource` registry; built-in synth voices and SoundFont (SF2) instruments via `rustysynth` |
| `bg-audio` | cpal streams, shared transport clock, sample-accurate output scheduler, analysis worker, `SessionLogic` trait, latency calibration |
| `bg-ui` | ECAM-style themes, `ExerciseType` / `PreferencePage` plug-in traits, settings store, shared widgets (tuner, beat grid, piano keyboard, tempo + groove panel, results) |
| `bg-exercises` | exercise types: Basic timing, Basic chords; `TimingSession` + `Accompaniment` for backing tracks |
| `bass-gym` | app shell: sidebar, preset bar, preferences window |
| `sf2pack` | tool: packs multi-sampled FLAC recordings into SoundFonts |

## Instruments (SoundFonts)

The sampled e-pianos are build artifacts (git-ignored): `assets/soundfonts/*.sf2` are made from the recordings in `samples/e-piano/`, one SoundFont per subfolder, files named `<set>--vel<velocity>-<note><octave>.flac`. After cloning (the recordings are in Git LFS: `git lfs pull`), build them once with:

```
cargo soundfonts      # alias for: cargo run -p sf2pack --release -- samples/e-piano assets/soundfonts --name E-Piano
```

Without them the app still runs; only the FM e-piano is offered.

The packer cuts the silence before each attack (so notes sound on time), trims tails at the noise floor (max 12 s, `--max-seconds`), normalizes each set as a whole (keeping the velocity layers' levels) and maps keys and velocities halfway between the recorded ones.

At startup the app registers every `.sf2` in, in this order: the user's folder (`<config dir>/soundfonts`), `soundfonts/` next to the executable, `../Resources/soundfonts` (macOS app bundle) and `assets/soundfonts` in the source tree. When packaging, copy `assets/soundfonts` next to the executable (or into the bundle's `Resources`). Fonts load on first use; the first preset of each file is played.

Debugging aid: `BASS_GYM_SCREENSHOT=<file.ppm>` saves the window after 2 s and quits; `BASS_GYM_SIZE=<w>x<h>`, `BASS_GYM_EXERCISE=<id>` and `BASS_GYM_PREFS=<page id>` set up what is shown.

## Extending

- **New exercise type:** implement `bg_ui::ExerciseType` (+ `PresetSchema`) in `bg-exercises` and add it to `bg_exercises::all()`. Timing-based exercises can reuse `TimingSession` and the shared widgets.
- **Preset changes:** additive fields only need `#[serde(default)]`. For renames or restructures, bump `current_version()` and add a `migrate_step`. Presets that can't be migrated are shown as incompatible and are never overwritten.
- **New sound:** implement `bg_sound::Voice` and add a `SoundSource` to `bg_sound::sources()` (built in) or `bg_sound::register()` (at runtime). A new SoundFont needs no code: drop the `.sf2` into one of the folders above.
- **New chord-compatibility rule:** implement `bg_core::harmony::ChordChecker` and add it to `checkers()`.
- **New progression / comp rhythm:** add an entry to `harmony::LIBRARY` (roman numerals, no quality markers, so it works in every mode) or `harmony::comp::PATTERNS` + `hits()`.
- **Backing for a timing exercise:** implement `timing_session::Accompaniment` (schedule sounds per planned bar, judge played pitches) and start a `TimingSession::with_accompaniment`.

## License

Copyright (C) 2026 Gyula Laszlo

bass-gym is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. It is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; see [LICENSE](LICENSE) for details.

This covers the code and the e-piano recordings in `samples/` (and the SoundFonts built from them).
