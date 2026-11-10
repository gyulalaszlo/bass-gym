# Audio engine

All of this is in `crates/bg-audio/src/` unless noted.

## Threads

| Thread | Entry | Notes |
|---|---|---|
| cpal input callback | `engine.rs::InputSink::process` | Real time |
| cpal output callback | `output.rs::OutputState::process` | Real time |
| `bg-analysis` worker | `worker.rs::run` | Plain thread polling every 2 ms, not real time |
| UI | `engine.poll()` each frame from `app/mod.rs` | Also frees old voices |

## Communication

| Link | Mechanism | Size |
|---|---|---|
| Input -> worker | rtrb `f32` samples + rtrb `InChunk` | `sr*2`, 1024 |
| Input -> output (monitor) | rtrb `f32` | `sr/2` |
| Worker -> output | rtrb `OutMsg` | 4096 |
| UI -> output | rtrb `ControlMsg` | 64 |
| Output -> UI (old voices, freed off-thread) | rtrb `Garbage` | 64 |
| UI -> worker | `std::sync::mpsc` `WorkerCmd` | unbounded |
| Worker -> UI | `Arc<Mutex>` tuner / meter, `AtomicBool` session_running | |
| Session -> UI | `Arc<Mutex<TimingShared>>`, `Arc<Mutex<ChordShared>>` | |
| Clock | Seqlock of atomics, written by the output callback | |

```rust
pub(crate) enum OutMsg { Play { at: f64, slot: Slot, ev: SoundEvent }, StopAll }
pub(crate) enum ControlMsg { Voice(Slot, Box<dyn Voice>), Master(f32),
    Monitor { enabled: bool, gain: f32 }, MonitorLowcut(Option<f32>),
    InstrumentLowpass(Option<f32>), MonitorSource(Consumer<f32>, u32) }
pub(crate) enum WorkerCmd { SetInput(Option<InputLink>), SetOutput(Option<Producer<OutMsg>>),
    SetAnalysis(AnalysisConfig), Start(Box<dyn SessionLogic>, Option<RecordSpec>), Stop, Shutdown }
```

`ControlMsg` can only be sent from the UI thread (the producer sits on `AudioEngine`),
not from `SessionIo`.

## Capture

- cpal 0.18 input stream, formats F32 / I16 / I32.
- Each frame becomes one mono f32: the selected channel, or a mean of all channels.
  `input_channel` defaults to `Some(0)`.
- Each callback pushes its samples to the worker and monitor rings, then one `InChunk`.
- If the worker ring lacks room, the whole callback is dropped and counted in the next
  chunk's `dropped`.

```rust
struct InputSink { samples: Producer<f32>, chunks: Producer<InChunk>,
    monitor: Producer<f32>, clock: Clock, channels: usize, channel: Option<usize> }
pub(crate) struct InChunk { pub frames: u32, pub time: Option<f64>, pub dropped: u32 }
```

## Worker loop (`worker.rs`)

1. Pop a chunk.
2. Update `a.offset`, the estimated transport time of input sample 0. It uses a 2% EMA
   and snaps on jumps of 50 ms or more.
3. Pop samples into `buf` and advance `consumed`.
4. `a.history.extend(&buf)`. `History` is a 2 s ring addressed by absolute sample index.
5. Update the meter, run the tuner, then `detector.process`.
6. Convert sample indices with `t(sample) = offset + sample / sr - latency`.
7. Call the session's `note_on`, `note_off` and `note_pitch`.

`bg_dsp::filter::History` is a separate type used by the tuner. The tap in step 4 sees
raw, unfiltered mono audio.

## Monitoring and mix (`output.rs`)

- `Monitor` reads the third ring and linearly resamples input rate to output rate. It
  targets ~10 ms queued, and past 10 + 40 ms it trims back. It discards samples, so it
  can't be reused for playback.
- `LowCut` is two biquads (4th-order Butterworth), driven by
  `SoundConfig::monitor_lowcut()`. It applies only in the mix, to the monitor signal.
- The mix runs in 16-frame blocks. Click and groove voices render into `mix`; the
  instrument renders into its own buffer, gets its optional lowpass and is added; then
  the monitor; then `soft_clip(m * master)`.
- The mono result goes to every output channel. There's no stereo.

## Sample rate

- Default is the device default, or `AudioConfig.sample_rate`.
- Input prefers the output rate if supported. A shared device is forced to the output's
  config.
- Input and output rates can still differ. The audio page shows a warning, and
  `InputLink.sample_rate` carries the actual input rate.

## Clock and latency (`clock.rs`)

Transport time is `output frame index / sample rate`, running since the stream opened,
not since the session. `Clock::now()` is the time currently heard. It returns 0 once
the clock is stopped, so the worker's `transport_now` keeps the last running value for
`SessionIo.now`.

Latency has three layers:
1. **Output:** `now()` subtracts `playback - callback`.
2. **Input:** `map_host_ns` maps the cpal capture time. On CoreAudio, capture is
   callback time minus buffer, device latency and safety offset.
3. **Residual:** `AnalysisConfig.latency_ms`, subtracted in the worker's `t(sample)`.
   It's measured by `calibration.rs::Calibration`, applied from `pages/latency.rs`, and read on
   each loop, so changes apply mid-session.

The monitor path delay isn't compensated. Without host timestamps the fallback is
`clock.now() - n / sr`, which has more jitter.

## Sound generation and scheduling

No MIDI runs at runtime. Everything is a `SoundEvent` scheduled at a transport time.

```rust
// bg-sound/src/lib.rs
pub enum SoundKind { Click(ClickKind), Note { midi: f32, velocity: f32, duration: f32, release: f32 } }
pub struct SoundEvent { pub kind: SoundKind, pub gain: f32 }
pub trait Voice: Send { fn trigger(&mut self, ev: &SoundEvent);
    fn render(&mut self, out: &mut [f32]); fn silence(&mut self); }
// session.rs
pub enum Slot { Click = 0, Groove = 1, Instrument = 2, Playback = 3 }
```

- Click uses the built-in `SineClick` / `Woodblock` / `PluckBass` voices (`bg-sound/src/synth.rs`).
- Chords default to `sf2:e-piano-mellow` through `rustysynth`, or `FmPiano`.
- `release` (Basic chords' Piano › Release) lets a `SoundFontVoice` note ring on after
  `duration`; the voice's next attack cuts every key still ringing. Synth voices ignore
  it. Recording and replay keep it, but zero it on notes they cut short.
- `Voice` needs no hardware, so voices can render offline.
- `TimingSession::tick` plans ahead: 0.35 s lookahead plus 2 bars. For each bar,
  `schedule()` sends clicks and groove notes through `io.play`, and
  `ChordAccompaniment::plan_bar` sends the comp (its seeded humanize can jitter times)
  and returns the attacks. Only after that does the bar's merged targets go to
  `matcher.add_bar` (see data-model.md, Targets).
- `SessionIo::play` is the single chokepoint for every scheduled sound, including
  calibration.
- The output converts `at` to a frame and triggers anything due before the block end,
  so events start up to 15 frames early. Overdue events fire immediately.
- `io.play`'s result is ignored by callers, and a full `pending` list silently drops
  events, so scheduled and played can differ.

## Session lifecycle

```rust
pub trait SessionLogic: Send {
    fn start(&mut self, io: &mut SessionIo);
    fn tick(&mut self, io: &mut SessionIo) -> SessionState;
    fn note_on(&mut self, time: f64, level_db: f32);
    fn note_pitch(&mut self, onset: f64, freq: f32) { .. }
    fn note_off(&mut self, time: f64, peak_db: f32, freq: Option<f32>);
    fn stop(&mut self, io: &mut SessionIo) { io.silence(); }
}
```

- **Start:** UI transport button -> `start()` in the exercise -> `engine.start_session`
  -> `WorkerCmd::Start`. The worker stops any old session, then calls `logic.start`.
  `TimingSession::start` sets `start = now + 0.3`.
- **Stop:** three paths in the worker: Start replacing a session, `WorkerCmd::Stop`, and
  `tick` returning `Finished`. App exit sends Stop too.
- The UI detects the end with `was_running && !running`.
- Calibration uses the same Start path (`pages/latency.rs`).

**Rebuilds.** `AudioEngine::rebuild` stops the session and sends `SetInput(None)`, which
drops `Analysis` and its `consumed` count. A new `OutputState` restarts `frames` at 0,
so a device failure or audio setting change kills the session and resets transport
time. Any change to `AudioConfig` triggers this.

## Recording (`recording.rs`, `bg-sound/src/sample.rs`)

`engine.start_recorded(logic, max_bytes) -> SessionId` records a session;
`start_session` doesn't, so calibration isn't recorded. `take_recording(id)` hands the
result over once, and `bg-ui/src/recording.rs::SessionCapture` wraps this for exercises.
The cap is `RecordingPrefs`, a separate settings table, because `AudioConfig` changes
reopen the devices.

- `Recorder` lives in the worker loop, outside `Analysis`, and is fed raw mono input at
  the history tap. `Env` bundles what `SessionIo` needs.
- `SampleWriter` keeps ~1 s blocks and drops the oldest past the cap, with min/max peaks
  built while writing. Memory grows with the session. `SampleBuffer` is the finished,
  immutable audio; indices are absolute, so they stay valid after overwriting.
- `Recording::time_of` / `sample_at` use anchors of `sample -> offset + n/sr - latency`,
  stored every 0.25 s and whenever the mapping moves by over 1 ms, so markers match note
  times by construction. Recorder indices differ from `Analysis.consumed` once silence
  has been inserted: always map through transport time.
- Lost input is reported on the next `InChunk.dropped`. The recorder fills it with
  silence and adds a `Gap`, and the worker snaps its offset.
- `SessionIo::play` logs `LoggedSound`s into the recorder. `Recorder::finish` drops
  sounds past the stop time and cuts notes short.
- `end_session` handles all three end paths and publishes before `session_running` goes
  false. `shared.running` can fall earlier, so exercises poll every frame.

## Replay (`replay.rs`)

`engine.start_replay(recording, from, to, bass_gain, handle)` runs a `ReplaySession`
like any other session, so `session_running` covers it and `stop_session` stops it. It
never touches `TimingShared`, so exercises don't see it as running.

- The bass plays through `Slot::Playback`: a `SampleVoice` pushed over `ControlMsg` first,
  triggered by a normal scheduled event. Control messages drain first in `process`, so
  the voice is in place by then. Call `release_replay` afterwards so it drops its `Arc`.
- Logged sounds that were delivered are re-scheduled with a transport `shift`, using the
  current voices. Notes overlapping the region start are re-emitted there, shortened.
- `ReplayHandle` carries the Click and Chords flags (Groove follows Click) and gives the
  playhead through `position(now)`.
- Quiet practice's low cut also applies to playback (`playback_lowcut`).
- A rebuild ends a replay like any session.

## Note detection (`bg-dsp/src/notes.rs`)

```rust
pub enum NoteEvent {
    /// `sample` counts from the first sample ever passed to the detector.
    On { sample: u64, level_db: f32 },
    Off { sample: u64, peak_db: f32 },
    Noise { sample: u64, end: u64, peak_db: f32 },
}
```

- **Hold stage (`settle`):** every onset is held `HOLD_MS` (40 ms) before it's
  reported, so the live view shows notes that much later (times stay exact).
  - If its peak stays `noise_db` (default 12) below the median of the last 8 notes, it's
    string noise: `Noise`, passed to `SessionLogic::note_noise` and kept in
    `Matcher.noise` for review, never graded. Real string noise was 15-25 dB down, ghost
    notes at most ~8 dB.
  - A note half that far down (6 dB) is a suspect: the sound of a mute, or of a finger
    landing, can be only ~9 dB down, but a much louder note follows within 60-100 ms.
    If one follows within `FOLLOW_MS` (110) and `FOLLOW_DB` (8) louder, the suspect is
    noise; otherwise it's reported as a note once that time has passed, so a quiet
    note is reported up to ~110 ms late.
  - A precursor (the finger meeting the string, before a muted pluck's thud or a real
    pluck) moves the onset: a hop 12 dB above everything since the onset, bar the last
    `JUMP_RISE_MS` (8 ms, longer than a low note's own attack), after at least
    `PRECURSOR_MS`, means the note starts where that rise left the precursor's level.
  - `tests/fixtures/string_noise.wav`, `mute_noise.wav` and `ghost_notes.wav` are clips
    of real playing for these; `examples/note_features.rs` prints per-note measurements of a report's WAV.

- **Time bases:** `NoteEvent` is in input samples since `Analysis` was created.
  `SessionLogic` gets transport seconds, latency compensated. Raw indices aren't passed
  on.
- **Pluck cue:** a high-band burst must beat the short window and also the bursts of the
  last 42 ms (`HF_PERIOD_RISE_DB`): low notes can have a sharp edge in every period, so
  bursts recurring at the same level aren't plucks. Too high a margin delays real notes
  by a few ms, because they then wait for the low band.
- **Dip cue:** the peak cue's reference spans 42 ms, a period of the lowest note, so it
  can't see a mute shorter than that, which is normal above ~100 BPM eighths.
  `PeriodTracker` measures the ringing note's period, and `dip_onset` looks for a rise
  within `DIP_RISE_MS` in the highest peak of one period. The period is cleared at each
  onset, so a note's first ~60 ms uses the peak cue only, and a lower note can't inherit
  a short window. Levels over different spans never compare (a longer one fakes a rise).
  The rise must return within `DIP_RETURN_DB` of the note's peak, which rejects mute noise.
  Limits: a mute shorter than about a period stays invisible, and beating that swings much
  more than 12 dB looks like a mute.
- **Checking against real audio:** `cargo run -p bg-dsp --example detect_wav --
  <report>/audio.wav [release_db]` prints what the detector finds. Clips of real playing
  are in `crates/bg-dsp/tests/fixtures`, tested by `tests/recorded.rs`.
- **Retroactive events:** `Off` is placed back by the 40 ms RMS window plus
  `end_offset_ms`. `On` from a high-band candidate can land up to ~15 ms before
  confirmation.
- **Pitch:** Hz, and not part of `NoteEvent`. Early attempts run at 80-250 ms after the
  onset. The final value comes from up to 600 ms of the note's own samples, with a
  tuner-median fallback.
- `Played.offset` stays `None` if a note is still sounding at stop.

## File formats

`crates/bg-report` writes reports: `audio.wav` (hound, float), `report.json` and
`click_chords.mid` (midly), into `<data dir>/reports/<stamp>_bars-a-b/` plus a `.zip`
(zip). Bars are padded by `PADDING`. Every time is seconds from the WAV start, found with
`Region::rel` through the recording's time map. MIDI uses one fixed tempo so event times
stay exact; real tempos are in the JSON. Bump `FORMAT_VERSION` if the JSON layout changes.
`bg_report::clear` backs the Clear report folder button; it only deletes items named like
reports (`is_report`).

`sf2pack` has private RIFF chunk writing and uses `claxon` for FLAC. There's no MP3
decoder.
