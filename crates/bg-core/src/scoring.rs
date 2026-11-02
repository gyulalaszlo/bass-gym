//! Matching detected notes against expected notes, grading, and aggregate statistics.

use serde::{Deserialize, Serialize};

use crate::plan::{BarPlan, Progress};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Grade {
    Good,
    Ok,
    Poor,
    Missed,
}

impl Grade {
    pub fn is_hit(self) -> bool {
        matches!(self, Grade::Good | Grade::Ok)
    }
}

/// How chord compatibility affects a note's grade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradeMode {
    /// A note's grade is the worst of its timing, length and harmony grades.
    #[default]
    Combined,
    /// The grade covers timing and length only; harmony is graded separately.
    Separate,
}

/// Grade of a chord-compatibility score (1 = chord tone, 0.5 = passing tone, 0 = clash).
pub fn harmony_grade(score: f32) -> Grade {
    if score >= 0.999 {
        Grade::Good
    } else if score >= 0.5 {
        Grade::Ok
    } else {
        Grade::Poor
    }
}

/// Evaluation tolerances. These are user preferences, not part of exercise presets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoringThresholds {
    pub good_ms: f32,
    pub ok_ms: f32,
    /// Notes further than this from any expected note count as extra notes.
    pub window_ms: f32,
    pub score_length: bool,
    /// Allowed relative length deviation for a good / ok grade.
    pub length_good: f32,
    pub length_ok: f32,
}

impl Default for ScoringThresholds {
    fn default() -> Self {
        Self {
            good_ms: 12.0,
            ok_ms: 30.0,
            window_ms: 150.0,
            score_length: true,
            length_good: 0.3,
            length_ok: 0.6,
        }
    }
}

impl ScoringThresholds {
    pub fn timing_grade(&self, ms: f32) -> Grade {
        let a = ms.abs();
        if a <= self.good_ms {
            Grade::Good
        } else if a <= self.ok_ms {
            Grade::Ok
        } else {
            Grade::Poor
        }
    }

    pub fn length_grade(&self, ratio: f32) -> Grade {
        let d = (ratio - 1.0).abs();
        if !self.score_length || d <= self.length_good {
            Grade::Good
        } else if d <= self.length_ok {
            Grade::Ok
        } else {
            Grade::Poor
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Expected {
    pub bar: i64,
    pub bpm: f32,
    pub time: f64,
    pub len: f64,
    pub accent: bool,
    /// Half-width of the matching window for this note, in seconds.
    pub window: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Played {
    pub onset: f64,
    pub offset: Option<f64>,
    pub peak_db: f32,
    pub freq: Option<f32>,
    /// Chord-compatibility score, when harmony is judged and the pitch is known.
    pub harmony: Option<f32>,
    /// Index of the matched expected note, `None` for an extra note.
    pub matched: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outcome {
    pub played: Option<usize>,
    pub timing_ms: Option<f32>,
    pub length_ratio: Option<f32>,
    pub harmony: Option<f32>,
    pub harmony_grade: Option<Grade>,
    /// The note's grade (see [`GradeMode`]).
    pub grade: Option<Grade>,
}

#[derive(Clone, Debug, Default)]
pub struct Matcher {
    pub thresholds: ScoringThresholds,
    /// Whether played pitches are judged against chords.
    pub harmony_enabled: bool,
    pub grade_mode: GradeMode,
    pub expected: Vec<Expected>,
    pub outcomes: Vec<Outcome>,
    pub played: Vec<Played>,
    /// Index of the first expected note that may still change state.
    first_open: usize,
    now: f64,
}

impl Matcher {
    pub fn new(thresholds: ScoringThresholds) -> Self {
        Self {
            thresholds,
            ..Default::default()
        }
    }

    /// A matcher that also grades harmony (see [`Matcher::on_pitch`]).
    pub fn with_harmony(thresholds: ScoringThresholds, grade_mode: GradeMode) -> Self {
        Self {
            thresholds,
            harmony_enabled: true,
            grade_mode,
            ..Default::default()
        }
    }

    fn regrade(&mut self, i: usize) {
        let th = &self.thresholds;
        let o = &mut self.outcomes[i];
        if o.played.is_none() {
            return;
        }
        let mut g = th.timing_grade(o.timing_ms.unwrap_or(0.0));
        if let Some(r) = o.length_ratio {
            g = g.max(th.length_grade(r));
        }
        if let (GradeMode::Combined, Some(h)) = (self.grade_mode, o.harmony_grade) {
            g = g.max(h);
        }
        o.grade = Some(g);
    }

    /// Records the chord-compatibility score of a played note (may be called again with
    /// a better estimate).
    pub fn on_pitch(&mut self, played: usize, harmony: Option<f32>) {
        let Some(p) = self.played.get_mut(played) else {
            return;
        };
        p.harmony = harmony;
        if let Some(i) = p.matched {
            self.outcomes[i].harmony = harmony;
            self.outcomes[i].harmony_grade = harmony.map(harmony_grade);
            self.regrade(i);
        }
    }

    pub fn add_bar(&mut self, bar: &BarPlan) {
        let max_window = self.thresholds.window_ms as f64 / 1000.0;
        for n in &bar.notes {
            self.expected.push(Expected {
                bar: bar.index,
                bpm: bar.bpm,
                time: n.time,
                len: n.len,
                accent: n.accent,
                window: max_window,
            });
            self.outcomes.push(Outcome::default());
        }
        // Each note's window extends at most half-way to its neighbours.
        let k = self.expected.len();
        for i in k.saturating_sub(bar.notes.len() + 1)..k {
            let mut w = max_window;
            if i > 0 {
                w = w.min((self.expected[i].time - self.expected[i - 1].time) / 2.0);
            }
            if i + 1 < k {
                w = w.min((self.expected[i + 1].time - self.expected[i].time) / 2.0);
            }
            self.expected[i].window = w;
        }
    }

    /// Registers a note onset; returns its index in `played`.
    pub fn on_onset(&mut self, onset: f64, peak_db: f32) -> usize {
        let mut best: Option<(usize, f64)> = None;
        for i in self.first_open..self.expected.len() {
            let e = &self.expected[i];
            if e.time - e.window > onset {
                break;
            }
            let d = (onset - e.time).abs();
            if self.outcomes[i].played.is_none()
                && d <= e.window
                && best.is_none_or(|(_, bd)| d < bd)
            {
                best = Some((i, d));
            }
        }
        let idx = self.played.len();
        self.played.push(Played {
            onset,
            offset: None,
            peak_db,
            freq: None,
            harmony: None,
            matched: best.map(|b| b.0),
        });
        if let Some((i, _)) = best {
            let ms = ((onset - self.expected[i].time) * 1000.0) as f32;
            self.outcomes[i] = Outcome {
                played: Some(idx),
                timing_ms: Some(ms),
                ..Default::default()
            };
            self.regrade(i);
        }
        idx
    }

    /// Completes a played note with its end time, final peak level and (optional) pitch.
    pub fn on_offset(&mut self, played: usize, offset: f64, peak_db: f32, freq: Option<f32>) {
        let Some(p) = self.played.get_mut(played) else {
            return;
        };
        p.offset = Some(offset);
        p.peak_db = peak_db;
        if freq.is_some() {
            p.freq = freq;
        }
        if let Some(i) = p.matched {
            self.outcomes[i].length_ratio =
                Some(((offset - p.onset) / self.expected[i].len) as f32);
            self.regrade(i);
        }
    }

    /// Advances time, marking expected notes whose window passed unplayed as missed.
    pub fn update(&mut self, now: f64) {
        self.now = now;
        while self.first_open < self.expected.len() {
            let e = &self.expected[self.first_open];
            if e.time + e.window >= now {
                break;
            }
            let o = &mut self.outcomes[self.first_open];
            if o.played.is_none() {
                o.grade = Some(Grade::Missed);
            }
            self.first_open += 1;
        }
    }

    /// Time of the latest input activity (end of the last note, or infinity while a
    /// note is still sounding); `None` before anything was played.
    pub fn last_activity(&self) -> Option<f64> {
        self.played
            .last()
            .map(|p| p.offset.unwrap_or(f64::INFINITY))
    }

    pub fn stats(&self) -> SessionStats {
        let mut s = SessionStats::default();
        for (e, o) in self.expected.iter().zip(&self.outcomes) {
            let Some(g) = o.grade else { continue };
            s.counts[g as usize] += 1;
            let tempo = match s.per_tempo.iter_mut().find(|t| t.bpm == e.bpm) {
                Some(t) => t,
                None => {
                    s.per_tempo.push(TempoStats {
                        bpm: e.bpm,
                        ..Default::default()
                    });
                    s.per_tempo.last_mut().unwrap()
                }
            };
            tempo.notes += 1;
            if g.is_hit() {
                tempo.hits += 1;
            }
            if let Some(ms) = o.timing_ms {
                s.timing_ms.push(ms);
                tempo.timing_ms.push(ms);
            }
            if let Some(r) = o.length_ratio {
                s.length_ratios.push(r);
            }
            if let Some(p) = o.played {
                let db = self.played[p].peak_db;
                if e.accent {
                    s.accent_levels_db.push(db)
                } else {
                    s.levels_db.push(db)
                }
                if self.harmony_enabled {
                    let k = match o.harmony {
                        Some(h) if h >= 0.999 => 0,
                        Some(h) if h >= 0.5 => 1,
                        Some(_) => 2,
                        None => 3,
                    };
                    s.harmony[k] += 1;
                }
            }
        }
        s.extra = self.played.iter().filter(|p| p.matched.is_none()).count();
        s
    }
}

impl Progress for Matcher {
    fn bar_rate(&self, bar: i64) -> Option<f32> {
        let mut n = 0;
        let mut hits = 0;
        for (e, o) in self
            .expected
            .iter()
            .zip(&self.outcomes)
            .filter(|(e, _)| e.bar == bar)
        {
            if e.time + e.window >= self.now {
                return None;
            }
            n += 1;
            if o.grade.is_some_and(Grade::is_hit) {
                hits += 1;
            }
        }
        (n > 0).then(|| hits as f32 / n as f32)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TempoStats {
    pub bpm: f32,
    pub notes: usize,
    pub hits: usize,
    pub timing_ms: Vec<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionStats {
    /// Indexed by `Grade as usize`.
    pub counts: [usize; 4],
    pub extra: usize,
    pub timing_ms: Vec<f32>,
    pub length_ratios: Vec<f32>,
    pub levels_db: Vec<f32>,
    pub accent_levels_db: Vec<f32>,
    pub per_tempo: Vec<TempoStats>,
    /// Played notes by chord compatibility: [chord tone, passing, clash, pitch unknown].
    pub harmony: [usize; 4],
}

impl SessionStats {
    pub fn count(&self, g: Grade) -> usize {
        self.counts[g as usize]
    }

    pub fn total(&self) -> usize {
        self.counts.iter().sum()
    }

    /// Share of played notes (with a known pitch) that were chord tones.
    pub fn harmony_rate(&self) -> Option<f32> {
        let known = self.harmony[0] + self.harmony[1] + self.harmony[2];
        (known > 0).then(|| self.harmony[0] as f32 / known as f32)
    }

    pub fn hit_rate(&self) -> Option<f32> {
        let t = self.total();
        (t > 0).then(|| (self.count(Grade::Good) + self.count(Grade::Ok)) as f32 / t as f32)
    }
}

pub fn mean(v: &[f32]) -> Option<f32> {
    (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
}

pub fn std_dev(v: &[f32]) -> Option<f32> {
    let m = mean(v)?;
    (v.len() > 1)
        .then(|| (v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / (v.len() - 1) as f32).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Planner;
    use crate::session::SessionConfig;

    fn setup() -> (Matcher, Vec<BarPlan>) {
        let cfg = SessionConfig {
            tempo: 120.0,
            count_in_bars: 0,
            ..Default::default()
        };
        let mut p = Planner::new(cfg, 0.0);
        let mut m = Matcher::new(ScoringThresholds::default());
        let bars: Vec<BarPlan> = (0..2).map(|_| p.next_bar(&()).unwrap()).collect();
        bars.iter().for_each(|b| m.add_bar(b));
        (m, bars)
    }

    #[test]
    fn grading_and_misses() {
        let (mut m, _) = setup();
        // Quarter notes at 0.0, 0.5, 1.0, ...
        let a = m.on_onset(0.005, -10.0);
        m.on_offset(a, 0.49, -10.0, None);
        let b = m.on_onset(0.52, -12.0);
        m.on_offset(b, 0.80, -12.0, None); // 0.56 of expected length -> ok
        let c = m.on_onset(1.2, -12.0); // extra (window is 150ms)
        m.on_offset(c, 1.3, -12.0, None);
        m.update(1.7);
        assert_eq!(m.outcomes[0].grade, Some(Grade::Good));
        assert_eq!(m.outcomes[1].grade, Some(Grade::Ok));
        assert_eq!(m.outcomes[2].grade, Some(Grade::Missed));
        assert_eq!(m.outcomes[3].grade, Some(Grade::Missed));
        assert_eq!(m.outcomes[4].grade, None);
        let s = m.stats();
        assert_eq!(s.extra, 1);
        assert_eq!(s.count(Grade::Missed), 2);
        assert!((mean(&s.timing_ms).unwrap() - 12.5).abs() < 1e-3);
        assert_eq!(m.bar_rate(0), Some(0.5));
        assert_eq!(m.bar_rate(1), None);
        m.update(10.0);
        assert_eq!(m.bar_rate(0), Some(0.5));
        assert_eq!(m.bar_rate(1), Some(0.0));
    }

    #[test]
    fn harmony_grading_modes() {
        for mode in [GradeMode::Combined, GradeMode::Separate] {
            let (m, _) = setup();
            let mut m = Matcher {
                harmony_enabled: true,
                grade_mode: mode,
                ..m
            };
            let a = m.on_onset(0.002, -10.0); // on time
            m.on_pitch(a, Some(0.5)); // passing tone
            let b = m.on_onset(0.501, -10.0);
            m.on_pitch(b, Some(0.0)); // clash
            m.on_pitch(b, Some(1.0)); // better estimate: chord tone
            let c = m.on_onset(1.02, -10.0); // ok timing
            m.on_pitch(c, Some(0.0));
            let grades: Vec<_> = m.outcomes[..3].iter().map(|o| o.grade.unwrap()).collect();
            match mode {
                GradeMode::Combined => assert_eq!(grades, [Grade::Ok, Grade::Good, Grade::Poor]),
                GradeMode::Separate => assert_eq!(grades, [Grade::Good, Grade::Good, Grade::Ok]),
            }
            assert_eq!(m.outcomes[2].harmony_grade, Some(Grade::Poor));
            let s = m.stats();
            assert_eq!(s.harmony, [1, 1, 1, 0]);
            assert!((s.harmony_rate().unwrap() - 1.0 / 3.0).abs() < 1e-6);
        }
    }

    #[test]
    fn nearest_note_wins() {
        let (mut m, _) = setup();
        m.on_onset(0.26, -10.0); // slightly closer to 0.5 than 0.0, window clamps to 0.25 => extra
        m.on_onset(0.48, -10.0);
        assert_eq!(m.played[0].matched, None);
        assert_eq!(m.played[1].matched, Some(1));
        assert_eq!(m.outcomes[1].grade, Some(Grade::Ok)); // -20ms
    }
}
