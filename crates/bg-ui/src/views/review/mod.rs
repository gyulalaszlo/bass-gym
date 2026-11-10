//! The Score + Audio tab of the results: what was played as a score over the recorded
//! waveform, with playback, a marker editor and bug-report export.
//!
//! A **sub-program**: like a tier-3 program it has effects (replay and export), so it
//! can't be a plain component. Its owner (an exercise) keeps the [`Review`] in its model
//! and routes the review's messages and effects:
//! - wraps [`Msg`] in its own `Msg`, and calls [`update`] with it;
//! - wraps [`Effect`] in its own `Effect`, and calls [`run`] with it;
//! - calls [`sync`] before showing the review, and sends [`Msg::Tick`] after the dialog.
//!
//! Files: `model.rs`, `msg.rs`, `update.rs` (pure, tested), `effects.rs` (the engine and
//! the export thread), `sync.rs`, `theme.rs`, and `view/` (controls, report row, score
//! rows, marker editor). The pure helpers are here.

mod effects;
mod model;
mod msg;
mod sync;
mod theme;
mod update;
mod view;

use bg_audio::{AnalysisConfig, AudioConfig, Recording, SoundConfig};
use bg_core::harmony::Scale;
use bg_core::score::{ScoreBar, ScoreChord, ScoreEvent, ScoreKind};
use bg_core::{BarPlan, Matcher};
use std::sync::Arc;

use crate::exercise::ExerciseCtx;
use crate::settings::{ANALYSIS, AUDIO, SOUNDS};
use crate::theme::{GradeColors, tokens};
use crate::views::marker_editor::{Line, Marker};
use crate::widgets::time_axis::TimeAxis;
use crate::widgets::waveform::PeakSource;

pub use effects::{Effect, run};
pub use model::Review;
pub use msg::Msg;
pub use sync::sync;
pub use theme::{Metrics, Theme};
pub use update::update;
pub use view::{Props, ReviewView};

/// How close to a note a click on the score counts as clicking it, in points.
const NOTE_REACH: f32 = 12.0;
/// Playback from a note starts this long before it, so its attack is heard whole.
const NOTE_LEAD: f64 = 0.02;

/// What a finished session leaves behind for review.
pub struct ReviewInput {
    /// Count-in included.
    pub bars: Vec<BarPlan>,
    pub matcher: Matcher,
    pub chords: Vec<ScoreChord>,
    pub scale: Option<Scale>,
    pub a4: f32,
    pub recording: Arc<Recording>,
    /// The exercise's id.
    pub exercise: String,
    /// Settings to put in reports; see [`report_settings`].
    pub settings: serde_json::Value,
}

/// The settings a report should record: the exercise's own, and those of the app.
pub fn report_settings(ctx: &ExerciseCtx, exercise: serde_json::Value) -> serde_json::Value {
    let status = ctx.engine.status();
    serde_json::json!({
        "exercise": exercise,
        "scoring": ctx.scoring,
        "analysis": ctx.settings.get::<AnalysisConfig>(ANALYSIS),
        "audio": ctx.settings.get::<AudioConfig>(AUDIO),
        "sounds": ctx.settings.get::<SoundConfig>(SOUNDS),
        "devices": {
            "input": status.input,
            "output": status.output,
            "input_rate": status.input_rate,
            "output_rate": status.output_rate,
        },
    })
}

/// A recording as a [`PeakSource`] on the transport timeline.
pub(crate) struct RecordingPeaks<'a>(pub &'a Recording);

impl PeakSource for RecordingPeaks<'_> {
    fn range(&self) -> (f64, f64) {
        (self.0.first_time(), self.0.end_time())
    }

    fn peak(&self, from: f64, to: f64) -> Option<(f32, f32)> {
        self.0
            .audio
            .peak_range(self.0.sample_at(from), self.0.sample_at(to))
    }
}

/// A position in the music: bar (from 1), beat (from 1) and 32nd note within the beat
/// (from 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicPosition {
    pub bar: i64,
    pub beat: u32,
    pub sub: u32,
}

/// The position of transport time `t` within `bars`.
pub fn position_at(bars: &[ScoreBar], t: f64) -> Option<MusicPosition> {
    let bar = bars.iter().rev().find(|b| b.start <= t).or(bars.first())?;
    let beats = bar.sig.num.max(1);
    let beat_len = (bar.end - bar.start) / beats as f64;
    let into = (t - bar.start).max(0.0);
    let beat = ((into / beat_len) as u32).min(beats - 1);
    let subs = (32 / bar.sig.den.max(1)).max(1);
    let sub = (((into - beat as f64 * beat_len) / beat_len * subs as f64) as u32).min(subs - 1);
    Some(MusicPosition {
        bar: bar.index + 1,
        beat: beat + 1,
        sub: sub + 1,
    })
}

/// Where playback should start for a click at `x` in `bar`: at the recorded note drawn
/// there, or else at the start of the bar.
pub(crate) fn click_start(bar: &ScoreBar, m: &Matcher, axis: TimeAxis, x: f32) -> f64 {
    let distance = |e: &ScoreEvent| (axis.x(e.time) - x).abs();
    bar.events
        .iter()
        .filter(|e| distance(e) <= NOTE_REACH)
        .filter_map(|e| match &e.kind {
            ScoreKind::Note(n) => Some((distance(e), m.played.get(n.played)?.onset)),
            _ => None,
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(bar.start, |(_, onset)| onset - NOTE_LEAD)
}

/// The expected, played and bar markers of a session.
/// Their colours are data (each marker carries one), taken from the current tokens.
pub(crate) fn overlays(m: &Matcher, bars: &[ScoreBar]) -> Vec<Marker> {
    let t = tokens();
    let grades = GradeColors::new(&t);
    let mut id = 0;
    let mut next = || {
        id += 1;
        id
    };
    let mut out = Vec::new();
    for b in bars {
        out.push(Marker {
            id: next(),
            time: b.start,
            end: None,
            label: format!("BAR {}", b.index + 1),
            color: t.dim(),
            line: Line::Dotted,
        });
    }
    for e in &m.expected {
        out.push(Marker {
            id: next(),
            time: e.time,
            end: None,
            label: String::new(),
            color: t.dim(),
            line: Line::Dotted,
        });
    }
    // String noise: faint, so it's there to check but doesn't read as a note.
    for n in &m.noise {
        out.push(Marker {
            id: next(),
            time: n.onset,
            end: Some(n.offset),
            label: String::new(),
            color: t.dim().gamma_multiply(0.6),
            line: Line::Dotted,
        });
    }
    for p in &m.played {
        let grade = p
            .matched
            .and_then(|i| m.outcomes.get(i))
            .and_then(|o| o.grade);
        out.push(Marker {
            id: next(),
            time: p.onset,
            end: Some(p.offset.unwrap_or(p.onset)),
            label: String::new(),
            color: grade.map_or(t.red(), |g| grades.get(g)),
            line: Line::Solid,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_core::TimeSignature;
    use egui::Rangef;

    fn bar(index: i64, start: f64) -> ScoreBar {
        ScoreBar {
            index,
            start,
            end: start + 2.0,
            bpm: 120.0,
            show_bpm: false,
            sig: TimeSignature::default(),
            events: Vec::new(),
        }
    }

    #[test]
    fn positions_count_bars_beats_and_32nds_from_one() {
        let bars = [bar(0, 10.0), bar(1, 12.0)];
        let at = |t| position_at(&bars, t).unwrap();
        assert_eq!(
            at(10.0),
            MusicPosition {
                bar: 1,
                beat: 1,
                sub: 1
            }
        );
        // 13.0 is beat 3 of bar 2; a 32nd of a 0.5 s beat is 62.5 ms.
        assert_eq!(
            at(13.0),
            MusicPosition {
                bar: 2,
                beat: 3,
                sub: 1
            }
        );
        assert_eq!(
            at(13.07),
            MusicPosition {
                bar: 2,
                beat: 3,
                sub: 2
            }
        );
        assert_eq!(
            at(13.99),
            MusicPosition {
                bar: 2,
                beat: 4,
                sub: 8
            }
        );
    }

    #[test]
    fn position_before_the_first_bar_is_its_start() {
        let bars = [bar(0, 10.0)];
        let p = position_at(&bars, 5.0).unwrap();
        assert_eq!((p.bar, p.beat, p.sub), (1, 1, 1));
        assert_eq!(position_at(&[], 5.0), None);
    }

    #[test]
    fn a_click_plays_from_the_note_there_or_the_start_of_the_bar() {
        use bg_core::score::{NoteValue, ScoreNote};
        let mut m = Matcher::default();
        for onset in [10.52, 11.03] {
            m.played.push(bg_core::scoring::Played {
                onset,
                offset: None,
                peak_db: -10.0,
                freq: None,
                harmony: None,
                matched: None,
            });
        }
        let event = |time, kind| ScoreEvent {
            time,
            beat: 0,
            value: NoteValue {
                denominator: 4,
                dots: 0,
            },
            tuplet: None,
            kind,
        };
        let note = |played| {
            ScoreKind::Note(ScoreNote {
                played,
                pitch: None,
                clash: false,
                chord_marks: Vec::new(),
                tied: false,
                continuation: false,
                stem_up: true,
                beam_group: None,
            })
        };
        let mut b = bar(0, 10.0);
        b.events = vec![
            event(10.5, note(0)),
            event(11.0, note(1)),
            event(11.5, ScoreKind::Rest),
        ];
        // 100 points per second, starting at the bar.
        let axis = TimeAxis::new(Rangef::new(0.0, 200.0), 10.0, 12.0);
        let start = |x| click_start(&b, &m, axis, x);
        // Notes are written on the grid, but play from where they really were.
        assert!((start(52.0) - (10.52 - NOTE_LEAD)).abs() < 1e-9);
        assert!((start(95.0) - (11.03 - NOTE_LEAD)).abs() < 1e-9);
        assert_eq!(start(20.0), 10.0);
        // A rest isn't a place of its own.
        assert_eq!(start(150.0), 10.0);
    }

    #[test]
    fn overlays_use_grade_colours_and_red_for_extras() {
        let mut m = Matcher::default();
        m.played.push(bg_core::scoring::Played {
            onset: 1.0,
            offset: Some(1.5),
            peak_db: -10.0,
            freq: None,
            harmony: None,
            matched: None,
        });
        let o = overlays(&m, &[bar(0, 0.0)]);
        assert_eq!(o.len(), 2);
        assert_eq!(o[1].color, crate::theme::red());
        assert_eq!(o[1].end, Some(1.5));
    }
}
