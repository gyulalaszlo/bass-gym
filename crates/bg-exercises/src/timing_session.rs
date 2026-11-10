//! Shared session logic for timing exercises: plans bars ahead of time, schedules the
//! click and groove click, and scores played notes against the groove. An optional
//! [`Accompaniment`] adds backing (e.g. a piano) and judges the played pitches.

use std::sync::{Arc, Mutex, MutexGuard};

use bg_audio::{SessionIo, SessionLogic, SessionState, Slot};
use bg_core::plan::{BarPlan, PlannedNote};
use bg_core::scoring::{Noise, Played};
use bg_core::{GradeMode, Matcher, Planner, RecentTiming, ScoringThresholds, SessionConfig};
use bg_sound::{SoundEvent, SoundKind};

/// Minimum scheduling lead for sounds.
const LOOKAHEAD: f64 = 0.35;
/// Bars are planned this many bars ahead, so the beat grid always shows what's coming
/// (tempo / click-gap decisions for a bar are made when it is planned).
const PLAN_AHEAD_BARS: f64 = 4.0;
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
    /// sounds through `io` and record what is played for the UI. Returns the attacks
    /// played in the bar, on the grid (before any humanizing), which can serve as targets.
    fn plan_bar(&mut self, bar: &BarPlan, io: &mut SessionIo) -> Vec<PlannedNote>;
    /// Compatibility (0..1) of a pitch played for the beat at `time`; `None` when there
    /// is nothing to judge against (e.g. during the count-in).
    fn judge(&self, time: f64, freq: f32) -> Option<f32>;
    /// The session ended at `now`: forget what was planned beyond it.
    fn stop(&mut self, now: f64) {
        let _ = now;
    }
}

/// What a session with an accompaniment scores.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccompanimentScoring {
    pub grade_mode: GradeMode,
    /// The groove's notes are targets.
    pub groove: bool,
    /// The accompaniment's attacks are targets.
    pub hits: bool,
    /// Played pitches are judged against the accompaniment.
    pub harmony: bool,
}

impl Default for AccompanimentScoring {
    fn default() -> Self {
        Self {
            grade_mode: GradeMode::Combined,
            groove: true,
            hits: false,
            harmony: true,
        }
    }
}

/// The targets of a bar: the groove's notes plus the accompaniment's attacks that aren't
/// within `tol` seconds of one. A merged target keeps the groove note's length and is
/// accented if either was.
pub fn merge_targets(groove: &[PlannedNote], hits: &[PlannedNote], tol: f64) -> Vec<PlannedNote> {
    let mut out = groove.to_vec();
    for h in hits {
        match out[..groove.len()]
            .iter_mut()
            .find(|g| (g.time - h.time).abs() <= tol)
        {
            Some(g) => g.accent |= h.accent,
            None => out.push(*h),
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

pub struct TimingSession {
    cfg: SessionConfig,
    accompaniment: Option<(Box<dyn Accompaniment>, AccompanimentScoring)>,
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
    pub fn new(
        cfg: SessionConfig,
        mut thresholds: ScoringThresholds,
        shared: SharedTiming,
    ) -> Self {
        thresholds.score_length = cfg.score_length;
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

    /// A session with backing, scoring what `scoring` asks for.
    pub fn with_accompaniment(
        cfg: SessionConfig,
        thresholds: ScoringThresholds,
        shared: SharedTiming,
        accompaniment: Box<dyn Accompaniment>,
        scoring: AccompanimentScoring,
    ) -> Self {
        Self {
            accompaniment: Some((accompaniment, scoring)),
            ..Self::new(cfg, thresholds, shared)
        }
    }

    /// Stores the pitch of played note `i` and, with an accompaniment, judges it at
    /// [`judge_time`]. The timing lock is released while judging: the accompaniment takes
    /// the chord lock, and the UI may hold that one first.
    fn judge(&self, i: usize, freq: f32) {
        let mut shared = lock(&self.shared);
        let m = &shared.matcher;
        let Some(p) = m.played.get(i) else { return };
        let time = judge_time(m, p);
        shared.matcher.played[i].freq = Some(freq);
        drop(shared);
        if let Some((acc, scoring)) = &self.accompaniment
            && scoring.harmony
        {
            let score = acc.judge(time, freq);
            lock(&self.shared).matcher.on_pitch(i, score);
        }
    }
}

impl SessionLogic for TimingSession {
    fn start(&mut self, io: &mut SessionIo) {
        let start = io.now + START_DELAY;
        let planner = Planner::new(self.cfg.clone(), start);
        self.planned_until = start;
        let matcher = match &self.accompaniment {
            Some((_, s)) if s.harmony => {
                Matcher::with_harmony(self.thresholds.clone(), s.grade_mode)
            }
            _ => Matcher::new(self.thresholds.clone()),
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
        let (groove_targets, hit_targets) = self
            .accompaniment
            .as_ref()
            .map_or((true, false), |(_, s)| (s.groove, s.hits));
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
                    drop(shared);
                    schedule(&self.cfg, io, &bar);
                    let hits = match self.accompaniment.as_mut() {
                        Some((acc, _)) => acc.plan_bar(&bar, io),
                        None => Vec::new(),
                    };
                    // The groove click still sounds from `bar`; the matcher gets the targets.
                    let scored = BarPlan {
                        notes: merge_targets(
                            if groove_targets { &bar.notes } else { &[] },
                            if hit_targets { &hits } else { &[] },
                            grid_tolerance(&bar),
                        ),
                        ..bar.clone()
                    };
                    shared = lock(&self.shared);
                    shared.matcher.add_bar(&scored);
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

    fn note_noise(&mut self, start: f64, end: f64, peak_db: f32) {
        if start >= self.scoring_from {
            lock(&self.shared).matcher.noise.push(Noise {
                onset: start,
                offset: end,
                peak_db,
            });
        }
    }

    fn stop(&mut self, io: &mut SessionIo) {
        io.silence();
        let mut shared = lock(&self.shared);
        shared.matcher.update(io.now);
        // Drop notes that were planned but never reached.
        let now = io.now;
        let keep = shared.matcher.expected.partition_point(|e| e.time <= now);
        shared.matcher.truncate_expected(keep);
        shared.bars.retain(|b| b.start <= now);
        shared.running = false;
        drop(shared);
        if let Some((acc, _)) = self.accompaniment.as_mut() {
            acc.stop(now);
        }
    }
}

/// How far after its onset an extra note is judged: a note played just ahead of a chord
/// change belongs to the new chord.
pub const EXTRA_JUDGE_DELAY: f64 = 0.03;

/// The time a played note's pitch is judged at: its target's time when matched, otherwise
/// shortly after its onset.
pub fn judge_time(m: &Matcher, p: &Played) -> f64 {
    p.matched
        .and_then(|e| m.expected.get(e))
        .map_or(p.onset + EXTRA_JUDGE_DELAY, |e| e.time)
}

/// A quarter of the bar's grid step: attacks closer than this are the same target.
fn grid_tolerance(bar: &BarPlan) -> f64 {
    bar.duration / (bar.beats.max(1) * bar.steps_per_beat.max(1)) as f64 / 4.0
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
                release: 0.0,
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

/// Height of the live performance strip above the beat grid.
pub const PERFORMANCE_HEIGHT: f32 = 64.0;

/// Timing of the last note and of the notes in the last `bars` bars (the bar at `now`
/// included), for the live performance indicator.
pub fn recent_timing(shared: &SharedTiming, now: f64, bars: u32) -> RecentTiming {
    let s = lock(shared);
    let Some(cur) = s.bars.iter().rposition(|b| b.start <= now) else {
        return RecentTiming::default();
    };
    let first = cur.saturating_sub(bars.max(1) as usize - 1);
    s.matcher.recent_timing(s.bars[first].start)
}

/// The timing meter of the performance strip: the running / last session's recent
/// timing, or an empty meter before the first session.
pub fn timing_meter(
    shared: &SharedTiming,
    has_results: bool,
    now: f64,
    bars: u32,
    th: &ScoringThresholds,
) -> bg_ui::views::performance::TimingMeter {
    bg_ui::views::performance::TimingMeter {
        recent: if has_results {
            recent_timing(shared, now, bars)
        } else {
            RecentTiming::default()
        },
        scale_ms: th.window_ms,
        good_ms: th.good_ms,
        ok_ms: th.ok_ms,
    }
}

/// A few bars of `cfg` from time 0, for previewing the groove before starting.
pub fn preview(cfg: &SessionConfig) -> (Vec<BarPlan>, Matcher) {
    preview_targets(cfg, true, |_| Vec::new())
}

/// Like [`preview`], with the groove's notes as targets or not, plus `hits(bar)` (merged
/// as a session merges an accompaniment's attacks).
pub fn preview_targets(
    cfg: &SessionConfig,
    groove: bool,
    hits: impl Fn(&BarPlan) -> Vec<PlannedNote>,
) -> (Vec<BarPlan>, Matcher) {
    let cfg = SessionConfig {
        count_in_bars: 0,
        ..cfg.clone()
    };
    let mut planner = Planner::new(cfg, 0.0);
    let mut matcher = Matcher::default();
    let bars: Vec<_> = (0..4).filter_map(|_| planner.next_bar(&())).collect();
    for b in &bars {
        let notes = merge_targets(
            if groove { &b.notes } else { &[] },
            &hits(b),
            grid_tolerance(b),
        );
        matcher.add_bar(&BarPlan { notes, ..b.clone() });
    }
    (bars, matcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(time: f64, len: f64, accent: bool) -> PlannedNote {
        PlannedNote { time, len, accent }
    }

    #[test]
    fn targets_merge_close_attacks() {
        let groove = [n(0.0, 0.5, false), n(1.0, 0.5, false)];
        let hits = [n(0.01, 2.0, true), n(0.75, 0.25, false), n(1.5, 0.5, true)];
        let t = merge_targets(&groove, &hits, 0.05);
        assert_eq!(
            t,
            [
                n(0.0, 0.5, true),
                n(0.75, 0.25, false),
                n(1.0, 0.5, false),
                n(1.5, 0.5, true)
            ]
        );
        assert_eq!(merge_targets(&[], &hits, 0.05), hits);
        assert_eq!(merge_targets(&groove, &[], 0.05), groove);
    }
}
