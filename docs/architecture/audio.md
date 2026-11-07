# Audio engine

All of this is in `crates/bg-audio/src/` unless noted.

## Threads

| Thread | Entry | Notes |
|---|---|---|
| cpal input callback | `engine.rs::InputSink::process` | Real time |
| cpal output callback | `output.rs::OutputState::process` | Real time |
| `bg-analysis` worker | `worker.rs::run` | Plain thread polling every 2 ms, not real time |
| UI | `engine.poll()` each frame from `app.rs` | Also frees old voices |

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
    SetAnalysis(AnalysisConfig), Start(Box<dyn SessionLogic>), Stop, Shutdown }
```

`ControlMsg` can only be sent from the UI thread (the producer sits on `AudioEngine`),
not from `SessionIo`.

## Capture

- cpal 0.18 input stream, formats F32 / I16 / I32.
- Each frame becomes one mono f32: the selected channel, or a mean of all channels.
  `input_channel` defaults to `Some(0)`.
- Each callback pushes one `InChunk` plus its samples to the worker, and the samples to
  the monitor ring.
- If the worker ring lacks room, the whole callback is dropped with no marker.

```rust
struct InputSink { samples: Producer<f32>, chunks: Producer<InChunk>,
    monitor: Producer<f32>, clock: Clock, channels: usize, channel: Option<usize> }
pub(crate) struct InChunk { pub frames: u32, pub time: Option<f64> }
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
   It's measured by `calibration.rs::Calibration`, applied from `pages.rs`, and read on
   each loop, so changes apply mid-session.

The monitor path delay isn't compensated. Without host timestamps the fallback is
`clock.now() - n / sr`, which has more jitter.

## Sound generation and scheduling

No MIDI runs at runtime. Everything is a `SoundEvent` scheduled at a transport time.

```rust
// bg-sound/src/lib.rs
pub enum SoundKind { Click(ClickKind), Note { midi: f32, velocity: f32, duration: f32 } }
pub struct SoundEvent { pub kind: SoundKind, pub gain: f32 }
pub trait Voice: Send { fn trigger(&mut self, ev: &SoundEvent);
    fn render(&mut self, out: &mut [f32]); fn silence(&mut self); }
// session.rs
pub enum Slot { Click = 0, Groove = 1, Instrument = 2 }
```

- Click uses the built-in `SineClick` / `Woodblock` / `PluckBass` voices (`bg-sound/src/synth.rs`).
- Chords default to `sf2:e-piano-mellow` through `rustysynth`, or `FmPiano`.
- `Voice` needs no hardware, so voices can render offline.
- `TimingSession::tick` plans ahead: 0.35 s lookahead plus 2 bars. `schedule()` sends
  clicks and groove notes through `io.play`. `ChordAccompaniment::plan_bar` sends the
  comp, and its seeded humanize can jitter times.
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
- Calibration uses the same Start path (`pages.rs`).

**Rebuilds.** `AudioEngine::rebuild` stops the session and sends `SetInput(None)`, which
drops `Analysis` and its `consumed` count. A new `OutputState` restarts `frames` at 0,
so a device failure or audio setting change kills the session and resets transport
time. Any change to `AudioConfig` triggers this.

## Note detection (`bg-dsp/src/notes.rs`)

```rust
pub enum NoteEvent {
    /// `sample` counts from the first sample ever passed to the detector.
    On { sample: u64, level_db: f32 },
    Off { sample: u64, peak_db: f32 },
}
```

- **Time bases:** `NoteEvent` is in input samples since `Analysis` was created.
  `SessionLogic` gets transport seconds, latency compensated. Raw indices aren't passed
  on.
- **Retroactive events:** `Off` is placed back by the 40 ms RMS window plus
  `end_offset_ms`. `On` from a high-band candidate can land up to ~15 ms before
  confirmation.
- **Pitch:** Hz, and not part of `NoteEvent`. Early attempts run at 80-250 ms after the
  onset. The final value comes from up to 600 ms of the note's own samples, with a
  tuner-median fallback.
- `Played.offset` stays `None` if a note is still sounding at stop.

## File formats

As of `a51c39f`, no code writes WAV or MIDI, and there's no zip or file dialog crate.
`sf2pack` has private RIFF chunk writing and uses `claxon` for FLAC. There's no MP3
decoder. `pages.rs::open_folder` is the only folder helper.
