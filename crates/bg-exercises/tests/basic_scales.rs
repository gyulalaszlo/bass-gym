//! Basic scales end to end, without audio hardware: a synthesized bassist plays C major
//! up, with one wrong note and one in the wrong octave, and the session grades them
//! through the real note and pitch detection.

use std::f32::consts::TAU;

use bg_audio::{SessionIo, SessionLogic, SessionState};
use bg_core::harmony::{Mode, Scale};
use bg_core::session::{AutoStop, Span, StopCondition, Trigger};
use bg_core::{Grade, Planner, ScoringThresholds, SessionConfig};
use bg_dsp::{NoteDetector, NoteDetectorConfig, NoteEvent};
use bg_exercises::basic_scales::{ScaleSettings, ScaleTargets, SharedScale, lock_scale};
use bg_exercises::timing_session::{AccompanimentScoring, SharedTiming, TimingSession, lock};

const SR: f32 = 48_000.0;

fn pluck(out: &mut [f32], start: f64, dur: f64, midi: i32) {
    let freq = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
    let s0 = (start * SR as f64) as usize;
    let len = ((dur + 0.03) * SR as f64) as usize;
    for i in 0..len.min(out.len().saturating_sub(s0)) {
        let t = i as f32 / SR;
        let release = if t as f64 > dur {
            (-(t - dur as f32) / 0.006).exp()
        } else {
            1.0
        };
        let env = (t / 0.002).min(1.0) * (-t / 1.0).exp() * release;
        let tone: f32 = (1..6)
            .map(|h| (TAU * freq * h as f32 * t).sin() / h as f32)
            .sum();
        out[s0 + i] += 0.4 * env * tone;
    }
}

#[test]
fn scale_session_grades_pitch() {
    use Grade::*;
    let cfg = SessionConfig {
        tempo: 120.0,
        count_in_bars: 1,
        key: Scale {
            root: 0,
            mode: Mode::Ionian,
        },
        auto_stop: AutoStop {
            enabled: true,
            when: StopCondition::Trigger(Trigger::After(Span::Bars(2))),
            ..Default::default()
        },
        ..Default::default() // quarter-note groove
    };
    // C D E F G, then B flat for A, B, and C an octave too high.
    let played = [36, 38, 40, 41, 43, 46, 47, 60];

    let scale = SharedScale::default();
    let timing = SharedTiming::default();
    let targets = ScaleTargets::new(
        ScaleSettings::default().sequence(&cfg.key, 0),
        440.0,
        scale.clone(),
    );
    let mut session = TimingSession::with_accompaniment(
        cfg.clone(),
        ScoringThresholds::default(),
        timing.clone(),
        Box::new(targets),
        AccompanimentScoring::default(),
    );
    let mut planner = Planner::new(cfg, 0.3);
    let times: Vec<f64> = std::iter::from_fn(|| planner.next_bar(&()))
        .filter(|b| !b.is_count_in())
        .flat_map(|b| b.notes)
        .map(|n| n.time)
        .collect();
    assert_eq!(times.len(), played.len());
    let mut audio = vec![0.0f32; ((times.last().unwrap() + 1.5) * SR as f64) as usize];
    for (&t, &midi) in times.iter().zip(&played) {
        pluck(&mut audio, t, 0.4, midi);
    }

    session.start(&mut SessionIo::offline(0.0, 0.0));
    let mut detector = NoteDetector::new(SR, NoteDetectorConfig::default());
    let mut events = Vec::new();
    let mut onset = 0u64;
    for (i, chunk) in audio.chunks(256).enumerate() {
        events.clear();
        detector.process(chunk, &mut events);
        for ev in &events {
            match *ev {
                NoteEvent::On { sample, level_db } => {
                    onset = sample;
                    session.note_on(sample as f64 / SR as f64, level_db);
                }
                NoteEvent::Off { sample, peak_db } => {
                    let from = onset as usize + (0.008 * SR) as usize;
                    let freq = bg_dsp::detect(&audio[from..sample as usize], SR).map(|r| r.freq);
                    if let Some(f) = freq {
                        session.note_pitch(onset as f64 / SR as f64, f);
                    }
                    session.note_off(sample as f64 / SR as f64, peak_db, freq);
                }
                NoteEvent::Noise { .. } => {}
            }
        }
        let now = ((i + 1) * 256) as f64 / SR as f64;
        if session.tick(&mut SessionIo::offline(now, 0.0)) == SessionState::Finished {
            session.stop(&mut SessionIo::offline(now, 0.0));
            break;
        }
    }

    let targets: Vec<u8> = lock_scale(&scale).targets.iter().map(|t| t.midi).collect();
    assert_eq!(targets, [36, 38, 40, 41, 43, 45, 47, 48]);
    let t = lock(&timing);
    let grades: Vec<Grade> = t
        .matcher
        .outcomes
        .iter()
        .map(|o| o.grade.unwrap())
        .collect();
    assert_eq!(grades, [Good, Good, Good, Good, Good, Poor, Good, Ok]);
}
