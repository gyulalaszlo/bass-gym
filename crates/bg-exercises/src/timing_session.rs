//! Shared session logic for timing exercises: plans bars ahead of time, schedules the
//! click and groove click, and scores played notes against the groove. An optional
//! [`Accompaniment`] adds backing (e.g. a piano) and judges the played pitches.

use std::sync::{Arc, Mutex, MutexGuard};

use bg_audio::{SessionIo, SessionLogic, SessionState, Slot};
use bg_core::plan::BarPlan;
use bg_core::{GradeMode, Matcher, Planner, ScoringThresholds, SessionConfig};
use bg_sound::{SoundEvent, SoundKind};

/// Minimum scheduling lead for sounds.
const LOOKAHEAD: f64 = 0.35;
/// Bars are planned this many bars ahead, so the beat grid always shows what's coming
/// (tempo / click-gap decisions for a bar are made when it is planned).
const PLAN_AHEAD_BARS: f64 = 2.0;
const START_DELAY: f64 = 0.3;

/// State shared between the session (worker thread) and the exercise UI.
#[derive(Default)]
pub struct TimingShared {
    pub bars: Vec<BarPlan>,
    pub matcher: Matcher,
    pub running: bool,
    /// Tempo of the bar currently planned.
    pub bpm: f32,
}

pub type SharedTiming = Arc<Mutex<TimingShared>>;

pub fn lock(shared: &SharedTiming) -> MutexGuard<'_, TimingShared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Backing played along with a timing session, which also judges played pitches.
/// Runs on the analysis worker thread.
pub trait Accompaniment: Send {
    /// Called once for every planned bar (count-in included), ahead of time: schedule
    /// sounds through `io` and record what is played for the UI.
    fn plan_bar(&mut self, bar: &BarPlan, io: &mut SessionIo);
    /// Compatibility (0..1) of a pitch played for the beat at `time`; `None` when there
    /// is nothing to judge against (e.g. during the count-in).
    fn judge(&self, time: f64, freq: f32) -> Option<f32>;
    /// The session ended at `now`: forget what was planned beyond it.
    fn stop(&mut self, now: f64) {
        let _ = now;
    }
}

pub struct TimingSession {
    cfg: SessionConfig,
    accompaniment: Option<(Box<dyn Accompaniment>, GradeMode)>,
    thresholds: ScoringThresholds,
    planner: Option<Planner>,
    shared: SharedTiming,
    planned_until: f64,
    scoring_from: f64,
    /// Start of the first (non count-in) bar.
    bar0_start: Option<f64>,
    active_note: Option<usize>,
}

impl TimingSession {
    pub fn new(cfg: SessionConfig, thresholds: ScoringThresholds, shared: SharedTiming) -> Self {
        Self {
            cfg,
            accompaniment: None,
            thresholds,
            planner: None,
            shared,
            planned_until: 0.0,
            scoring_from: f64::INFINITY,
            bar0_start: None,
            active_note: None,
        }
    }

    /// A session with backing whose judged pitches are graded according to `grade_mode`.
    pub fn with_accompaniment(
        cfg: SessionConfig,
        thresholds: ScoringThresholds,
        shared: SharedTiming,
        accompaniment: Box<dyn Accompaniment>,
        grade_mode: GradeMode,
    ) -> Self {
        Self {
            accompaniment: Some((accompaniment, grade_mode)),
            ..Self::new(cfg, thresholds, shared)
        }
    }

    /// Judges the pitch of played note `i` (at its target's time when matched).
    fn judge(&self, i: usize, freq: f32) {
        let Some((acc, _)) = &self.accompaniment else {
            return;
        };
        let mut shared = lock(&self.shared);
        let m = &shared.matcher;
        let Some(p) = m.played.get(i) else { return };
        let time = p.matched.map_or(p.onset, |e| m.expected[e].time);
        let score = acc.judge(time, freq);
        shared.matcher.played[i].freq = Some(freq);
        shared.matcher.on_pitch(i, score);
    }
}

impl SessionLogic for TimingSession {
    fn start(&mut self, io: &mut SessionIo) {
        let start = io.now + START_DELAY;
        let planner = Planner::new(self.cfg.clone(), start);
        self.planned_until = start;
        let matcher = match &self.accompaniment {
            Some((_, mode)) => Matcher::with_harmony(self.thresholds.clone(), *mode),
            None => Matcher::new(self.thresholds.clone()),
        };
        *lock(&self.shared) = TimingShared {
            matcher,
            running: true,
            bpm: planner.current_bpm(),
            bars: Vec::new(),
        };
        self.planner = Some(planner);
    }

    fn tick(&mut self, io: &mut SessionIo) -> SessionState {
        let Some(planner) = self.planner.as_mut() else {
            return SessionState::Finished;
        };
        let mut shared = lock(&self.shared);
        shared.matcher.update(io.now);
        loop {
            let bar_len = shared.bars.last().map_or(0.0, |b| b.duration);
            if self.planned_until >= io.now + LOOKAHEAD + PLAN_AHEAD_BARS * bar_len {
                break;
            }
            match planner.next_bar(&shared.matcher) {
                Some(bar) => {
                    if bar.index == 0 {
                        self.scoring_from = bar.start - self.thresholds.window_ms as f64 / 1000.0;
                        self.bar0_start = Some(bar.start);
                    }
                    self.planned_until = bar.end();
                    shared.bpm = bar.bpm;
                    shared.matcher.add_bar(&bar);
                    drop(shared);
                    schedule(&self.cfg, io, &bar);
                    if let Some((acc, _)) = self.accompaniment.as_mut() {
                        acc.plan_bar(&bar, io);
                    }
                    shared = lock(&self.shared);
                    shared.bars.push(bar);
                }
                None => break,
            }
        }
        // Stop when the player stopped: nothing played for N bars (counted from the first
        // bar, so not playing at all also stops the session).
        let stop = &self.cfg.auto_stop;
        if stop.on_silence
            && let Some(bar0) = self.bar0_start
        {
            let since = shared.matcher.last_activity().map_or(bar0, |t| t.max(bar0));
            let bar_len = shared
                .bars
                .iter()
                .rev()
                .find(|b| b.start <= io.now)
                .map_or(0.0, |b| b.duration);
            if bar_len > 0.0 && io.now - since >= stop.silence_bars.max(1) as f64 * bar_len {
                return SessionState::Finished;
            }
        }
        // Finished once auto-stop ended planning and the last bar (plus scoring window) passed.
        if planner.is_finished() && io.now > self.planned_until + 0.3 {
            SessionState::Finished
        } else {
            SessionState::Running
        }
    }

    fn note_on(&mut self, time: f64, level_db: f32) {
        if time < self.scoring_from {
            return; // Count-in.
        }
        self.active_note = Some(lock(&self.shared).matcher.on_onset(time, level_db));
    }

    fn note_pitch(&mut self, _onset: f64, freq: f32) {
        if let Some(i) = self.active_note {
            self.judge(i, freq);
        }
    }

    fn note_off(&mut self, time: f64, peak_db: f32, freq: Option<f32>) {
        if let Some(i) = self.active_note.take() {
            lock(&self.shared).matcher.on_offset(i, time, peak_db, freq);
            if let Some(f) = freq {
                self.judge(i, f);
            }
        }
    }

    fn stop(&mut self, io: &mut SessionIo) {
        io.silence();
        let mut shared = lock(&self.shared);
        shared.matcher.update(io.now);
        // Drop notes that were planned but never reached.
        let now = io.now;
        let keep = shared.matcher.expected.partition_point(|e| e.time <= now);
        shared.matcher.expected.truncate(keep);
        shared.matcher.outcomes.truncate(keep);
        shared.bars.retain(|b| b.start <= now);
        shared.running = false;
        drop(shared);
        if let Some((acc, _)) = self.accompaniment.as_mut() {
            acc.stop(now);
        }
    }
}

fn schedule(cfg: &SessionConfig, io: &mut SessionIo, bar: &BarPlan) {
    for c in &bar.clicks {
        io.play(
            c.time,
            Slot::Click,
            SoundEvent {
                kind: SoundKind::Click(c.kind),
                gain: cfg.click.volume,
            },
        );
    }
    let gc = &cfg.groove_click;
    if gc.enabled && !bar.muted {
        for n in &bar.notes {
            let kind = SoundKind::Note {
                midi: gc.pitch as f32,
                velocity: if n.accent { 1.0 } else { 0.75 },
                duration: (n.len * 0.9) as f32,
            };
            io.play(
                n.time,
                Slot::Groove,
                SoundEvent {
                    kind,
                    gain: gc.volume,
                },
            );
        }
    }
}

/// The end of a finished session, for showing its final state.
pub fn last_time(shared: &SharedTiming) -> f64 {
    let s = lock(shared);
    let last_note = s.matcher.played.last().map(|p| p.offset.unwrap_or(p.onset));
    let last_bar = s.bars.last().map(|b| b.end());
    match (last_note, last_bar) {
        (Some(n), Some(b)) => n.max(b - 0.001).min(b),
        (n, b) => n.or(b).unwrap_or(0.0),
    }
}

/// A few bars of `cfg` from time 0, for previewing the groove before starting.
pub fn preview(cfg: &SessionConfig) -> (Vec<BarPlan>, Matcher) {
    let cfg = SessionConfig {
        count_in_bars: 0,
        ..cfg.clone()
    };
    let mut planner = Planner::new(cfg, 0.0);
    let mut matcher = Matcher::default();
    let bars: Vec<_> = (0..4).filter_map(|_| planner.next_bar(&())).collect();
    bars.iter().for_each(|b| matcher.add_bar(b));
    (bars, matcher)
}
