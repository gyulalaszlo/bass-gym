//! Bar-by-bar session planning: turns a [`SessionConfig`] into concrete, timed bars,
//! applying count-in, tempo ramps, click gaps and auto-stop.

use crate::session::{ClickGap, SessionConfig, Span, StopCondition, Trigger};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickKind {
    Accent,
    Beat,
    Sub,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedClick {
    pub time: f64,
    pub kind: ClickKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedNote {
    pub time: f64,
    pub len: f64,
    pub accent: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BarPlan {
    /// Bar index; negative for count-in bars.
    pub index: i64,
    pub start: f64,
    pub duration: f64,
    pub bpm: f32,
    pub beats: u32,
    /// Clicks are silenced in this bar by the click-gap cycle.
    pub muted: bool,
    /// Clicks to play (already filtered by `muted` and the click config).
    pub clicks: Vec<PlannedClick>,
    /// Notes the player is expected to play (empty during count-in).
    pub notes: Vec<PlannedNote>,
}

impl BarPlan {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }

    pub fn is_count_in(&self) -> bool {
        self.index < 0
    }
}

/// Feedback the planner needs for success-based triggers.
pub trait Progress {
    /// Hit rate in 0..=1 of a fully scored bar, or `None` if not (yet) known / no notes.
    fn bar_rate(&self, bar: i64) -> Option<f32>;
}

impl Progress for () {
    fn bar_rate(&self, _: i64) -> Option<f32> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GapPhase {
    Play,
    Mute,
}

pub struct Planner {
    cfg: SessionConfig,
    start: f64,
    next_index: i64,
    next_time: f64,
    bpm: f32,
    tempo_since_bar: i64,
    tempo_since_time: f64,
    ramp_done_at: Option<i64>,
    gap_phase: GapPhase,
    gap_since_bar: i64,
    gap_since_time: f64,
    finished: bool,
}

impl Planner {
    pub fn new(cfg: SessionConfig, start: f64) -> Self {
        let count_in = cfg.count_in_bars as i64;
        let bpm = cfg.tempo.clamp(20.0, 400.0);
        Self {
            cfg,
            start,
            next_index: -count_in,
            next_time: start,
            bpm,
            tempo_since_bar: 0,
            tempo_since_time: 0.0,
            ramp_done_at: None,
            gap_phase: GapPhase::Play,
            gap_since_bar: 0,
            gap_since_time: 0.0,
            finished: false,
        }
    }

    pub fn config(&self) -> &SessionConfig {
        &self.cfg
    }

    pub fn current_bpm(&self) -> f32 {
        self.bpm
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Plans the next bar, or returns `None` once auto-stop decided the session is over.
    pub fn next_bar(&mut self, progress: &dyn Progress) -> Option<BarPlan> {
        if self.finished {
            return None;
        }
        let index = self.next_index;
        let start = self.next_time;
        let elapsed = start - self.start_of_bar0();

        if index >= 0 {
            self.update_tempo(index, elapsed, progress);
            if self.should_stop(index, elapsed, progress) {
                self.finished = true;
                return None;
            }
        }
        let muted = index >= 0 && self.update_gap(index, elapsed);

        let sig = self.cfg.time_sig;
        let spt = sig.seconds_per_tick(self.bpm);
        let duration = sig.ticks_per_bar() as f64 * spt;

        let mut clicks = Vec::new();
        let click = &self.cfg.click;
        if index < 0 || (click.enabled && !muted) {
            let per_beat = if index < 0 {
                1
            } else {
                click.density.per_beat()
            };
            let tpb = sig.ticks_per_beat();
            let (step, count) = match tpb.checked_div(per_beat) {
                Some(step) => (step, sig.num * per_beat),
                None => (sig.ticks_per_bar(), 1),
            };
            for i in 0..count {
                let pos = i * step;
                let kind = if pos == 0 && (click.accent_downbeat || index < 0) {
                    ClickKind::Accent
                } else if pos % tpb == 0 {
                    ClickKind::Beat
                } else {
                    ClickKind::Sub
                };
                clicks.push(PlannedClick {
                    time: start + pos as f64 * spt,
                    kind,
                });
            }
        }

        let notes = if index < 0 {
            Vec::new()
        } else {
            self.cfg
                .groove
                .bar_notes(sig, index as u32)
                .into_iter()
                .map(|n| PlannedNote {
                    time: start + n.pos as f64 * spt,
                    len: n.len as f64 * spt,
                    accent: n.accent,
                })
                .collect()
        };

        self.next_index += 1;
        self.next_time = start + duration;
        Some(BarPlan {
            index,
            start,
            duration,
            bpm: self.bpm,
            beats: sig.num,
            muted,
            clicks,
            notes,
        })
    }

    fn start_of_bar0(&self) -> f64 {
        // Count-in bars are always at the initial tempo, so bar 0's start is known up front.
        let sig = self.cfg.time_sig;
        let bar =
            sig.ticks_per_bar() as f64 * sig.seconds_per_tick(self.cfg.tempo.clamp(20.0, 400.0));
        self.start + self.cfg.count_in_bars as f64 * bar
    }

    fn update_tempo(&mut self, index: i64, elapsed: f64, progress: &dyn Progress) {
        let ramp = &self.cfg.ramp;
        if !ramp.enabled || self.ramp_done_at.is_some() {
            return;
        }
        if (self.bpm - ramp.target).abs() < 1e-3 {
            self.ramp_done_at = Some(index);
            return;
        }
        let fire = trigger_fired(
            ramp.every,
            index,
            self.tempo_since_bar,
            elapsed - self.tempo_since_time,
            progress,
        );
        if fire {
            let step = ramp.step.abs().max(0.1);
            self.bpm = if ramp.target > self.bpm {
                (self.bpm + step).min(ramp.target)
            } else {
                (self.bpm - step).max(ramp.target)
            };
            self.tempo_since_bar = index;
            self.tempo_since_time = elapsed;
            if (self.bpm - ramp.target).abs() < 1e-3 {
                self.ramp_done_at = Some(index);
            }
        }
    }

    fn should_stop(&self, index: i64, elapsed: f64, progress: &dyn Progress) -> bool {
        let stop = &self.cfg.auto_stop;
        if !stop.enabled {
            return false;
        }
        match stop.when {
            StopCondition::Trigger(t) => trigger_fired(t, index, 0, elapsed, progress),
            StopCondition::RampDone { bars } => match (self.cfg.ramp.enabled, self.ramp_done_at) {
                (true, Some(at)) => index - at >= bars as i64,
                (false, _) => index >= bars as i64,
                _ => false,
            },
        }
    }

    /// Returns whether the clicks of this bar are muted.
    fn update_gap(&mut self, index: i64, elapsed: f64) -> bool {
        let ClickGap {
            enabled,
            play,
            mute,
        } = self.cfg.gap;
        if !enabled {
            return false;
        }
        let span = if self.gap_phase == GapPhase::Play {
            play
        } else {
            mute
        };
        if span_elapsed(
            span,
            index - self.gap_since_bar,
            elapsed - self.gap_since_time,
        ) {
            self.gap_phase = if self.gap_phase == GapPhase::Play {
                GapPhase::Mute
            } else {
                GapPhase::Play
            };
            self.gap_since_bar = index;
            self.gap_since_time = elapsed;
        }
        self.gap_phase == GapPhase::Mute
    }
}

fn span_elapsed(span: Span, bars: i64, seconds: f64) -> bool {
    match span {
        Span::Bars(n) => bars >= n.max(1) as i64,
        // Small epsilon so that whole-bar multiples of the span don't miss by rounding.
        Span::Seconds(s) => seconds + 1e-6 >= s.max(0.1) as f64,
    }
}

fn trigger_fired(
    trigger: Trigger,
    index: i64,
    since_bar: i64,
    since_seconds: f64,
    progress: &dyn Progress,
) -> bool {
    match trigger {
        Trigger::After(span) => span_elapsed(span, index - since_bar, since_seconds),
        Trigger::Success { bars, min_rate } => {
            let bars = bars.max(1) as i64;
            if index - since_bar < bars {
                return false;
            }
            // Only consider fully scored bars at the current setting; the bar right before
            // `index` may still be in progress.
            let mut rates = Vec::new();
            let mut b = index - 1;
            while b >= since_bar && rates.len() < bars as usize {
                if let Some(r) = progress.bar_rate(b) {
                    rates.push(r);
                }
                b -= 1;
            }
            rates.len() == bars as usize && rates.iter().sum::<f32>() / bars as f32 >= min_rate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{AutoStop, StopCondition, TempoRamp};

    fn plan_all(cfg: SessionConfig, max: usize) -> Vec<BarPlan> {
        let mut p = Planner::new(cfg, 1.0);
        std::iter::from_fn(|| p.next_bar(&())).take(max).collect()
    }

    #[test]
    fn count_in_and_timing() {
        let cfg = SessionConfig {
            tempo: 120.0,
            count_in_bars: 1,
            ..Default::default()
        };
        let bars = plan_all(cfg, 3);
        assert_eq!(bars[0].index, -1);
        assert!(bars[0].notes.is_empty());
        assert_eq!(bars[0].clicks.len(), 4);
        assert_eq!(bars[1].index, 0);
        assert!((bars[1].start - 3.0).abs() < 1e-9);
        assert_eq!(bars[1].notes.len(), 4);
        assert!((bars[1].notes[1].time - 3.5).abs() < 1e-9);
        assert_eq!(bars[1].clicks[0].kind, ClickKind::Accent);
        assert_eq!(bars[1].clicks[1].kind, ClickKind::Beat);
    }

    #[test]
    fn ramp_every_n_bars_and_stop_when_done() {
        let cfg = SessionConfig {
            tempo: 100.0,
            count_in_bars: 0,
            ramp: TempoRamp {
                enabled: true,
                target: 110.0,
                step: 5.0,
                every: Trigger::After(Span::Bars(2)),
            },
            auto_stop: AutoStop {
                enabled: true,
                when: StopCondition::RampDone { bars: 2 },
                ..Default::default()
            },
            ..Default::default()
        };
        let bpms: Vec<f32> = plan_all(cfg, 100).iter().map(|b| b.bpm).collect();
        assert_eq!(bpms, vec![100.0, 100.0, 105.0, 105.0, 110.0, 110.0]);
    }

    #[test]
    fn ramp_down() {
        let cfg = SessionConfig {
            tempo: 100.0,
            count_in_bars: 0,
            ramp: TempoRamp {
                enabled: true,
                target: 92.0,
                step: 5.0,
                every: Trigger::After(Span::Bars(1)),
            },
            ..Default::default()
        };
        let bpms: Vec<f32> = plan_all(cfg, 4).iter().map(|b| b.bpm).collect();
        assert_eq!(bpms, vec![100.0, 95.0, 92.0, 92.0]);
    }

    #[test]
    fn click_gap_cycle() {
        let cfg = SessionConfig {
            count_in_bars: 1,
            gap: ClickGap {
                enabled: true,
                play: Span::Bars(2),
                mute: Span::Bars(1),
            },
            ..Default::default()
        };
        let muted: Vec<bool> = plan_all(cfg, 8).iter().map(|b| b.muted).collect();
        assert_eq!(
            muted,
            vec![false, false, false, true, false, false, true, false]
        );
        let cfg = SessionConfig {
            tempo: 120.0,
            count_in_bars: 0,
            gap: ClickGap {
                enabled: true,
                play: Span::Seconds(4.0),
                mute: Span::Seconds(2.0),
            },
            ..Default::default()
        };
        let bars = plan_all(cfg, 6);
        let muted: Vec<bool> = bars.iter().map(|b| b.muted).collect();
        assert_eq!(muted, vec![false, false, true, false, false, true]);
        assert!(bars[2].clicks.is_empty());
        assert!(!bars[2].notes.is_empty());
    }

    #[test]
    fn auto_stop_after_seconds() {
        let cfg = SessionConfig {
            tempo: 120.0,
            count_in_bars: 2,
            auto_stop: AutoStop {
                enabled: true,
                when: StopCondition::Trigger(Trigger::After(Span::Seconds(8.0))),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(plan_all(cfg, 100).len(), 2 + 4);
    }

    struct Rates(Vec<f32>);
    impl Progress for Rates {
        fn bar_rate(&self, bar: i64) -> Option<f32> {
            self.0.get(bar as usize).copied()
        }
    }

    #[test]
    fn success_trigger() {
        let cfg = SessionConfig {
            tempo: 100.0,
            count_in_bars: 0,
            ramp: TempoRamp {
                enabled: true,
                target: 200.0,
                step: 10.0,
                every: Trigger::Success {
                    bars: 2,
                    min_rate: 0.9,
                },
            },
            ..Default::default()
        };
        let rates = Rates(vec![0.5, 1.0, 0.9, 1.0, 1.0, 1.0, 1.0]);
        let mut p = Planner::new(cfg, 0.0);
        let bpms: Vec<f32> = (0..7).map(|_| p.next_bar(&rates).unwrap().bpm).collect();
        // bar 3 sees bars 1,2 => avg 0.95 -> step; bar 5 sees bars 3,4 -> step.
        assert_eq!(bpms, vec![100.0, 100.0, 100.0, 110.0, 110.0, 120.0, 120.0]);
    }
}
