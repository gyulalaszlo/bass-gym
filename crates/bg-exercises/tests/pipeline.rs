//! End-to-end check without audio hardware: a synthesized "player" with known timing
//! errors is run through the note detector and scored by the timing session.

use std::f32::consts::TAU;

use bg_audio::{Recorder, SessionIo, SessionLogic, SessionState, Slot};
use bg_core::scoring::{mean, std_dev};
use bg_core::session::{AutoStop, Span, StopCondition, Trigger};
use bg_core::{Grade, Planner, ScoringThresholds, SessionConfig};
use bg_dsp::{NoteDetector, NoteDetectorConfig, NoteEvent};
use bg_exercises::timing_session::{SharedTiming, TimingSession, lock};

const SR: f32 = 48_000.0;

fn pluck(out: &mut [f32], start: f64, dur: f64, freq: f32) {
    let s0 = (start * SR as f64) as usize;
    let mut seed = s0 as u32 | 1;
    let len = ((dur + 0.03) * SR as f64) as usize;
    for i in 0..len.min(out.len().saturating_sub(s0)) {
        let t = i as f32 / SR;
        let env = (t / 0.002).min(1.0)
            * (-t / 1.0).exp()
            * if t as f64 > dur {
                (-(t - dur as f32) / 0.006).exp()
            } else {
                1.0
            };
        let tone: f32 = (1..6)
            .map(|h| (TAU * freq * h as f32 * t).sin() / h as f32)
            .sum();
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let click = ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * (-t / 0.0015).exp() * 0.3;
        out[s0 + i] += 0.4 * (env * tone + click);
    }
}

#[test]
fn injected_timing_errors_are_measured() {
    // 120 BPM eighth notes, 1 bar count-in, stop after 1 bar.
    let cfg = SessionConfig {
        tempo: 120.0,
        count_in_bars: 1,
        groove: bg_core::groove::builtin_grooves(Default::default())[1].clone(),
        auto_stop: AutoStop {
            enabled: true,
            when: StopCondition::Trigger(Trigger::After(Span::Bars(1))),
            ..Default::default()
        },
        ..Default::default()
    };
    // The session starts 0.3 s after "now"; an identical planner tells us the note times.
    let mut planner = Planner::new(cfg.clone(), 0.3);
    let expected: Vec<(f64, f64)> = std::iter::from_fn(|| planner.next_bar(&()))
        .flat_map(|b| b.notes)
        .map(|n| (n.time, n.len))
        .collect();
    assert_eq!(expected.len(), 8);

    let shared = SharedTiming::default();
    let mut session = TimingSession::new(cfg, ScoringThresholds::default(), shared.clone());
    session.start(&mut SessionIo::offline(0.0, 0.0));

    // The player: errors in ms per note; the 6th note is skipped, plus one extra note.
    let errors_ms = [0.0, 6.0, -9.0, 18.0, -25.0, f64::NAN, 40.0, 3.0];
    let total = expected.last().unwrap().0 + 1.5;
    let mut audio = vec![0.0f32; (total * SR as f64) as usize];
    for (&(time, len), &err) in expected.iter().zip(&errors_ms) {
        if !err.is_nan() {
            pluck(&mut audio, time + err / 1000.0, len * 0.85, 41.2);
        }
    }
    pluck(&mut audio, expected[7].0 + 0.23, 0.01, 55.0); // 230 ms after the last note: outside its 125 ms window

    let mut detector = NoteDetector::new(SR, NoteDetectorConfig::default());
    let mut events = Vec::new();
    let block = 256;
    let mut state = SessionState::Running;
    for (i, chunk) in audio.chunks(block).enumerate() {
        events.clear();
        detector.process(chunk, &mut events);
        for ev in &events {
            match *ev {
                NoteEvent::On { sample, level_db } => {
                    session.note_on(sample as f64 / SR as f64, level_db)
                }
                NoteEvent::Off { sample, peak_db } => {
                    session.note_off(sample as f64 / SR as f64, peak_db, None)
                }
            }
        }
        let now = ((i + 1) * block) as f64 / SR as f64;
        state = session.tick(&mut SessionIo::offline(now, 0.0));
        if state == SessionState::Finished {
            session.stop(&mut SessionIo::offline(now, 0.0));
            break;
        }
    }
    assert_eq!(state, SessionState::Finished);

    let s = lock(&shared);
    let outcomes = &s.matcher.outcomes;
    for (i, (&err, o)) in errors_ms.iter().zip(outcomes).enumerate() {
        if err.is_nan() {
            assert_eq!(o.grade, Some(Grade::Missed), "note {i}");
        } else {
            let measured = o
                .timing_ms
                .unwrap_or_else(|| panic!("note {i} not matched: {o:?}"));
            assert!(
                (measured as f64 - err).abs() < 2.5,
                "note {i}: measured {measured} ms, injected {err} ms"
            );
        }
    }
    let grades: Vec<Grade> = outcomes.iter().map(|o| o.grade.unwrap()).collect();
    assert_eq!(
        &grades[..5],
        &[Grade::Good, Grade::Good, Grade::Good, Grade::Ok, Grade::Ok]
    );
    assert_eq!(grades[5], Grade::Missed);
    assert_eq!(grades[6], Grade::Poor);
    let stats = s.matcher.stats();
    assert_eq!(stats.extra, 1);
    assert_eq!(stats.count(Grade::Missed), 1);
    let injected: Vec<f32> = errors_ms
        .iter()
        .filter(|e| !e.is_nan())
        .map(|&e| e as f32)
        .collect();
    assert!((mean(&stats.timing_ms).unwrap() - mean(&injected).unwrap()).abs() < 1.5);
    assert!((std_dev(&stats.timing_ms).unwrap() - std_dev(&injected).unwrap()).abs() < 1.5);
    // Notes were held for 85% of their written length (plus a ~6 ms damped tail).
    let len = mean(&stats.length_ratios).unwrap();
    assert!((len - 0.85).abs() < 0.06, "length ratio {len}");
}

#[test]
fn next_bar_is_always_planned() {
    let cfg = SessionConfig {
        tempo: 120.0,
        count_in_bars: 1,
        ..Default::default()
    };
    let shared = SharedTiming::default();
    let mut session = TimingSession::new(cfg, ScoringThresholds::default(), shared.clone());
    session.start(&mut SessionIo::offline(0.0, 0.0));
    // 2 s bars starting at 0.3 s. Late in every bar, the following bar must be known.
    for now in [0.5, 2.2, 4.1, 6.25, 8.2] {
        session.tick(&mut SessionIo::offline(now, 0.0));
        let s = lock(&shared);
        let current = s.bars.iter().rposition(|b| b.start <= now).unwrap();
        assert!(
            s.bars.len() >= current + 2,
            "at {now}: {} bars planned",
            s.bars.len()
        );
        assert!(s.bars.last().unwrap().end() >= now + 2.0, "at {now}");
    }
}

#[test]
fn stops_when_player_stops() {
    let mut cfg = SessionConfig {
        tempo: 120.0,
        count_in_bars: 1,
        ..Default::default()
    };
    cfg.auto_stop.on_silence = true;
    cfg.auto_stop.silence_bars = 2;
    let shared = SharedTiming::default();
    let mut session = TimingSession::new(cfg, ScoringThresholds::default(), shared.clone());
    session.start(&mut SessionIo::offline(0.0, 0.0));
    // Bar 0 starts at 2.3 s; bars are 2 s. Play during bars 0-1, then stop at 6.0 s.
    let mut t = 0.0;
    let mut played_until = 0.0;
    let mut last_end = 0.0;
    let mut finished_at = None;
    while t < 20.0 {
        if (2.3..6.0).contains(&t) && t - played_until >= 0.5 {
            session.note_on(t, -10.0);
            session.note_off(t + 0.3, -10.0, None);
            played_until = t;
            last_end = t + 0.3;
        }
        if session.tick(&mut SessionIo::offline(t, 0.0)) == SessionState::Finished {
            finished_at = Some(t);
            break;
        }
        t += 0.01;
    }
    // 2 bars (4 s) of silence after the last note ended.
    let at = finished_at.expect("session should stop");
    assert!(
        (at - (last_end + 4.0)).abs() < 0.02,
        "stopped at {at}, last note ended {last_end}"
    );

    // Never playing at all stops 2 bars after the count-in.
    let mut cfg = SessionConfig {
        tempo: 120.0,
        count_in_bars: 1,
        ..Default::default()
    };
    cfg.auto_stop.on_silence = true;
    cfg.auto_stop.silence_bars = 2;
    let mut session =
        TimingSession::new(cfg, ScoringThresholds::default(), SharedTiming::default());
    session.start(&mut SessionIo::offline(0.0, 0.0));
    let mut t = 0.0;
    while session.tick(&mut SessionIo::offline(t, 0.0)) == SessionState::Running && t < 20.0 {
        t += 0.01;
    }
    assert!((t - 6.3).abs() < 0.05, "stopped at {t}");
}

#[test]
fn early_pitch_is_stored_without_an_accompaniment() {
    let cfg = SessionConfig {
        tempo: 120.0,
        count_in_bars: 0,
        ..Default::default()
    };
    let shared = SharedTiming::default();
    let mut session = TimingSession::new(cfg, ScoringThresholds::default(), shared.clone());
    session.start(&mut SessionIo::offline(0.0, 0.0));
    session.tick(&mut SessionIo::offline(0.0, 0.0));
    session.note_on(0.3, -10.0);
    session.note_pitch(0.3, 41.2);
    assert_eq!(lock(&shared).matcher.played[0].freq, Some(41.2));
}

#[test]
fn recorded_audio_lines_up_with_detected_notes() {
    // A pluck at a known place, fed in chunks the way the worker does, with latency
    // subtracted and a transport offset. The onset's time must map back to its sample.
    let (offset, latency) = (12.5, 0.02);
    let mut audio = vec![0.0f32; SR as usize * 2];
    pluck(&mut audio, 0.75, 0.5, 41.2);

    let mut detector = NoteDetector::new(SR, NoteDetectorConfig::default());
    let mut recorder = Recorder::new(usize::MAX);
    let mut events = Vec::new();
    let mut fed = 0;
    for chunk in audio.chunks(512) {
        let time = offset + fed as f64 / SR as f64 - latency;
        recorder.feed(chunk, SR, 0, Some(time));
        detector.process(chunk, &mut events);
        fed += chunk.len();
    }
    let recording = recorder.finish(offset + 2.0, latency);

    let sample = events
        .iter()
        .find_map(|e| match *e {
            NoteEvent::On { sample, .. } => Some(sample),
            _ => None,
        })
        .expect("the pluck is detected");
    let note_time = offset + sample as f64 / SR as f64 - latency;
    assert_eq!(recording.sample_at(note_time), sample);
    assert!((recording.time_of(sample) - note_time).abs() < 1e-9);
    let heard = sample as f64 / SR as f64;
    assert!((heard - 0.75).abs() < 0.03, "onset at {heard}");
}

#[test]
fn a_recorded_session_logs_its_click() {
    let cfg = SessionConfig {
        tempo: 120.0,
        count_in_bars: 0,
        ..Default::default()
    };
    let shared = SharedTiming::default();
    let mut session = TimingSession::new(cfg, ScoringThresholds::default(), shared);
    let mut log = Vec::new();
    session.start(&mut SessionIo::offline_logged(0.0, 0.0, &mut log));
    session.tick(&mut SessionIo::offline_logged(0.0, 0.0, &mut log));
    assert!(log.iter().any(|s| s.slot == Slot::Click));
    assert!(log.iter().all(|s| !s.delivered)); // Nothing to play it on.
}
