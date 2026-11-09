//! The analysis worker thread: consumes captured input, runs the tuner and the note
//! detector, maps everything onto the transport timeline and drives the active session.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bg_dsp::{NoteDetector, NoteEvent, PitchReading, PitchTracker, db};
use rtrb::{Consumer, Producer};

use crate::clock::Clock;
use crate::config::AnalysisConfig;
use crate::output::OutMsg;
use crate::recording::{Recorder, Recording, SessionId};
use crate::session::{SessionIo, SessionLogic, SessionState};

/// When to try detecting the pitch of a sounding note (ms after its onset): three
/// periods of low E fit in ~75 ms, low B needs a little more.
const PITCH_ATTEMPTS_MS: [f64; 4] = [80.0, 120.0, 170.0, 250.0];
/// Skip the attack transient.
const PITCH_SKIP_MS: f64 = 8.0;
/// Analyse at most this much of a note at note-off.
const PITCH_MAX_MS: f64 = 600.0;

/// Ring buffer of the most recent input samples, addressed by absolute sample index.
struct History {
    buf: Vec<f32>,
    total: u64,
}

impl History {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            total: 0,
        }
    }

    fn extend(&mut self, samples: &[f32]) {
        let n = self.buf.len();
        for &s in samples {
            self.buf[(self.total % n as u64) as usize] = s;
            self.total += 1;
        }
    }

    /// Samples `from..to` (absolute indices), if still held.
    fn slice(&self, from: u64, to: u64) -> Option<Vec<f32>> {
        let n = self.buf.len() as u64;
        let to = to.min(self.total);
        if from >= to || from + n < self.total {
            return None;
        }
        Some((from..to).map(|i| self.buf[(i % n) as usize]).collect())
    }
}

pub(crate) struct InChunk {
    pub frames: u32,
    /// Transport time of the chunk's first frame, if the host provided timestamps.
    pub time: Option<f64>,
    /// Frames lost just before this chunk because the worker fell behind.
    pub dropped: u32,
}

pub(crate) struct InputLink {
    pub samples: Consumer<f32>,
    pub chunks: Consumer<InChunk>,
    pub sample_rate: f32,
}

pub(crate) enum WorkerCmd {
    SetInput(Option<InputLink>),
    SetOutput(Option<Producer<OutMsg>>),
    SetAnalysis(AnalysisConfig),
    Start(Box<dyn SessionLogic>, Option<RecordSpec>),
    Stop,
    Shutdown,
}

pub(crate) type FinishedRecording = (SessionId, Arc<Recording>);

/// Asks for a session to be recorded.
pub(crate) struct RecordSpec {
    pub id: SessionId,
    pub max_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TunerState {
    pub reading: Option<PitchReading>,
    pub updated: Option<Instant>,
}

#[derive(Clone, Copy, Debug)]
pub struct InputMeter {
    pub peak_db: f32,
    pub rms_db: f32,
    pub note_active: bool,
    /// Length of the most recently completed note, in ms.
    pub last_note_ms: Option<f32>,
}

impl Default for InputMeter {
    fn default() -> Self {
        Self {
            peak_db: -120.0,
            rms_db: -120.0,
            note_active: false,
            last_note_ms: None,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct WorkerShared {
    pub tuner: Arc<Mutex<TunerState>>,
    pub meter: Arc<Mutex<InputMeter>>,
    pub session_running: Arc<AtomicBool>,
    /// The latest finished recording, until fetched.
    pub recording: Arc<Mutex<Option<FinishedRecording>>>,
}

struct Analysis {
    link: InputLink,
    tracker: PitchTracker,
    detector: NoteDetector,
    /// Samples consumed so far.
    consumed: u64,
    /// Estimated transport time of input sample 0.
    offset: Option<f64>,
    recent: Vec<f32>,
    note_freqs: Vec<f32>,
    note_on: bool,
    note_start: u64,
    /// Recent input, for detecting the pitch of the current note from its own samples.
    history: History,
    /// Index into `PITCH_ATTEMPTS_MS` of the next early pitch attempt.
    pitch_attempt: usize,
    pitch_sent: bool,
    meter_peak: f32,
    meter_energy: f32,
    meter_n: usize,
}

pub(crate) fn run(rx: Receiver<WorkerCmd>, clock: Clock, shared: WorkerShared) {
    let mut cfg = AnalysisConfig::default();
    let mut analysis: Option<Analysis> = None;
    let mut out: Option<Producer<OutMsg>> = None;
    let mut session: Option<Box<dyn SessionLogic>> = None;
    let mut buf: Vec<f32> = Vec::with_capacity(8192);
    let mut events: Vec<NoteEvent> = Vec::new();
    let mut last_now = 0.0;
    let mut recorder: Option<(SessionId, Recorder)> = None;

    loop {
        loop {
            match rx.try_recv() {
                Ok(WorkerCmd::SetInput(link)) => {
                    analysis = link.map(|link| {
                        let sr = link.sample_rate;
                        let mut tracker = PitchTracker::new(sr);
                        tracker.gate_db = cfg.tuner_gate_db;
                        Analysis {
                            link,
                            tracker,
                            detector: NoteDetector::new(sr, cfg.detector.clone()),
                            consumed: 0,
                            offset: None,
                            recent: Vec::new(),
                            note_freqs: Vec::new(),
                            note_on: false,
                            note_start: 0,
                            history: History::new((sr * 2.0) as usize),
                            pitch_attempt: 0,
                            pitch_sent: false,
                            meter_peak: 0.0,
                            meter_energy: 0.0,
                            meter_n: 0,
                        }
                    });
                }
                Ok(WorkerCmd::SetOutput(p)) => out = p,
                Ok(WorkerCmd::SetAnalysis(c)) => {
                    if let Some(a) = analysis.as_mut() {
                        a.detector.set_config(c.detector.clone());
                        a.tracker.gate_db = c.tuner_gate_db;
                    }
                    cfg = c;
                }
                Ok(WorkerCmd::Start(mut logic, spec)) => {
                    let mut env = Env {
                        clock: &clock,
                        cfg: &cfg,
                        out: &mut out,
                        last_now: &mut last_now,
                        recorder: &mut recorder,
                    };
                    end_session(&mut session, &mut env, &shared);
                    *env.recorder = spec.map(|s| (s.id, Recorder::new(s.max_bytes)));
                    logic.start(&mut env.io());
                    session = Some(logic);
                    shared.session_running.store(true, Ordering::Release);
                }
                Ok(WorkerCmd::Stop) => {
                    let mut env = Env {
                        clock: &clock,
                        cfg: &cfg,
                        out: &mut out,
                        last_now: &mut last_now,
                        recorder: &mut recorder,
                    };
                    end_session(&mut session, &mut env, &shared);
                    shared.session_running.store(false, Ordering::Release);
                }
                Ok(WorkerCmd::Shutdown) | Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }

        if let Some(a) = analysis.as_mut() {
            let latency = cfg.total_latency();
            while let Ok(chunk) = a.link.chunks.pop() {
                let n = chunk.frames as usize;
                let sr = a.link.sample_rate as f64;
                // Track the input→transport mapping; smooth out timestamp jitter, but follow jumps.
                let est = chunk
                    .time
                    .or_else(|| clock.is_running().then(|| clock.now() - n as f64 / sr))
                    .map(|t| t - a.consumed as f64 / sr);
                if let Some(est) = est {
                    a.offset = Some(match a.offset {
                        // Lost frames shift the estimate for good, so follow it at once.
                        Some(o) if (est - o).abs() < 0.05 && chunk.dropped == 0 => {
                            o + 0.02 * (est - o)
                        }
                        _ => est,
                    });
                }
                buf.clear();
                for _ in 0..n {
                    match a.link.samples.pop() {
                        Ok(s) => buf.push(s),
                        Err(_) => break,
                    }
                }
                if let Some((_, r)) = recorder.as_mut() {
                    let time = a.offset.map(|o| o + a.consumed as f64 / sr - latency);
                    r.feed(&buf, a.link.sample_rate, chunk.dropped, time);
                }
                a.consumed += buf.len() as u64;
                a.history.extend(&buf);

                for &s in &buf {
                    a.meter_peak = a.meter_peak.max(s.abs());
                    a.meter_energy += s * s;
                }
                a.meter_n += buf.len();

                if let Some(r) = a.tracker.push(&buf) {
                    publish_tuner(&shared, &mut a.recent, r);
                    if let (true, Some(r)) = (a.note_on, r) {
                        a.note_freqs.push(r.freq);
                    }
                }

                events.clear();
                a.detector.process(&buf, &mut events);
                let Some(offset) = a.offset else { continue };
                let t = |sample: u64| offset + sample as f64 / sr - latency;
                let skip = (PITCH_SKIP_MS / 1000.0 * sr) as u64;
                for ev in &events {
                    match *ev {
                        NoteEvent::On { sample, level_db } => {
                            a.note_on = true;
                            a.note_start = sample;
                            a.note_freqs.clear();
                            a.pitch_attempt = 0;
                            a.pitch_sent = false;
                            if let Some(s) = session.as_mut() {
                                s.note_on(t(sample), level_db);
                            }
                        }
                        NoteEvent::Off { sample, peak_db } => {
                            a.note_on = false;
                            if let Ok(mut m) = shared.meter.lock() {
                                m.last_note_ms = Some(
                                    sample.saturating_sub(a.note_start) as f32 / sr as f32 * 1000.0,
                                );
                            }
                            // The note's own samples give the most reliable pitch; tuner
                            // readings may still contain the previous note.
                            let end =
                                sample.min(a.note_start + (PITCH_MAX_MS / 1000.0 * sr) as u64);
                            let freq = a
                                .history
                                .slice(a.note_start + skip, end)
                                .and_then(|x| bg_dsp::detect(&x, sr as f32))
                                .map(|r| r.freq)
                                .or_else(|| median(&mut a.note_freqs));
                            if let Some(s) = session.as_mut() {
                                s.note_off(t(sample), peak_db, freq);
                            }
                        }
                        NoteEvent::Noise {
                            sample,
                            end,
                            peak_db,
                        } => {
                            if let Some(s) = session.as_mut() {
                                s.note_noise(t(sample), t(end), peak_db);
                            }
                        }
                    }
                }
                // Early pitch of the sounding note, as soon as enough of it was heard.
                if a.note_on && !a.pitch_sent && a.pitch_attempt < PITCH_ATTEMPTS_MS.len() {
                    let due =
                        a.note_start + (PITCH_ATTEMPTS_MS[a.pitch_attempt] / 1000.0 * sr) as u64;
                    if a.consumed >= due {
                        a.pitch_attempt += 1;
                        let found = a
                            .history
                            .slice(a.note_start + skip, a.consumed)
                            .and_then(|x| bg_dsp::detect(&x, sr as f32));
                        if let Some(r) = found {
                            a.pitch_sent = true;
                            if let Some(s) = session.as_mut() {
                                s.note_pitch(t(a.note_start), r.freq);
                            }
                        }
                    }
                }
            }
            if a.meter_n >= 1024 {
                if let Ok(mut m) = shared.meter.lock() {
                    m.peak_db = db(a.meter_peak);
                    m.rms_db = db((a.meter_energy / a.meter_n as f32).sqrt());
                    m.note_active = a.detector.is_note_active();
                }
                a.meter_peak = 0.0;
                a.meter_energy = 0.0;
                a.meter_n = 0;
            }
        }

        let mut env = Env {
            clock: &clock,
            cfg: &cfg,
            out: &mut out,
            last_now: &mut last_now,
            recorder: &mut recorder,
        };
        if let Some(logic) = session.as_mut()
            && logic.tick(&mut env.io()) == SessionState::Finished
        {
            end_session(&mut session, &mut env, &shared);
            shared.session_running.store(false, Ordering::Release);
        }

        std::thread::sleep(Duration::from_millis(2));
    }
}

/// What building a [`SessionIo`] needs from the worker loop.
struct Env<'a> {
    clock: &'a Clock,
    cfg: &'a AnalysisConfig,
    out: &'a mut Option<Producer<OutMsg>>,
    last_now: &'a mut f64,
    recorder: &'a mut Option<(SessionId, Recorder)>,
}

impl Env<'_> {
    fn io(&mut self) -> SessionIo<'_> {
        SessionIo {
            now: transport_now(self.clock, self.last_now),
            latency: self.cfg.total_latency(),
            out: if self.clock.is_running() {
                self.out.as_mut()
            } else {
                None
            },
            log: self.recorder.as_mut().map(|(_, r)| r.sounds_mut()),
        }
    }
}

/// Stops the running session, if any, and publishes its recording. Every way a session
/// ends comes through here, before `session_running` goes false, so the UI finds the
/// recording as soon as it sees the session has ended.
fn end_session(session: &mut Option<Box<dyn SessionLogic>>, env: &mut Env, shared: &WorkerShared) {
    let Some(mut logic) = session.take() else {
        return;
    };
    let mut io = env.io();
    logic.stop(&mut io);
    let (now, latency) = (io.now, io.latency);
    if let Some((id, r)) = env.recorder.take()
        && let Ok(mut slot) = shared.recording.lock()
    {
        *slot = Some((id, Arc::new(r.finish(now, latency))));
    }
}

/// The current transport time, or the last one seen once output has stopped (the clock
/// then reads 0, which would make a session ending in a device rebuild look like it
/// never started). Reads the time first: if the clock stops in between, it isn't kept.
fn transport_now(clock: &Clock, last_now: &mut f64) -> f64 {
    let now = clock.now();
    if clock.is_running() {
        *last_now = now;
    }
    *last_now
}

fn publish_tuner(shared: &WorkerShared, recent: &mut Vec<f32>, r: Option<PitchReading>) {
    let reading = r.map(|mut r| {
        // Median of the last few readings, restarted when the note changes.
        if recent
            .last()
            .is_some_and(|&f| (1200.0 * (r.freq / f).log2()).abs() > 50.0)
        {
            recent.clear();
        }
        recent.push(r.freq);
        if recent.len() > 5 {
            recent.remove(0);
        }
        let mut v = recent.clone();
        r.freq = median(&mut v).unwrap_or(r.freq);
        r
    });
    if reading.is_none() {
        recent.clear();
    }
    if let Ok(mut t) = shared.tuner.lock() {
        t.reading = reading;
        t.updated = Some(Instant::now());
    }
}

fn median(v: &mut [f32]) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[v.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ClockSnapshot;

    #[test]
    fn transport_time_survives_the_clock_stopping() {
        let clock = Clock::default();
        let mut last = 0.0;
        clock.publish(ClockSnapshot {
            frames: 96_000,
            sample_rate: 48_000,
            ..Default::default()
        });
        let running = transport_now(&clock, &mut last);
        assert!(running >= 2.0);
        clock.set_stopped();
        assert_eq!(clock.now(), 0.0);
        assert_eq!(transport_now(&clock, &mut last), running);
    }
}
