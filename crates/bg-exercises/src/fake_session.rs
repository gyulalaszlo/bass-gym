//! A made-up session for looking at the review without playing: a synthesized player
//! with timing errors, wrong notes, a missed note and stretches of eighths, run offline
//! through the real session logic. Set `BASS_GYM_FAKE_SESSION` to use it.

use std::f32::consts::TAU;

use bg_audio::{Recorder, Recording, SessionIo, SessionLogic, SessionState};
use bg_core::SessionConfig;
use bg_core::music::midi_to_freq;
use bg_core::session::{AutoStop, Span, StopCondition, Trigger};

use crate::timing_session::{SharedTiming, lock};

const SR: f32 = 48_000.0;
const STEP: f64 = 0.01;
/// Timing errors in seconds, cycled through.
const ERRORS: [f64; 5] = [0.004, -0.02, 0.015, -0.006, 0.042];
const PITCHES: [i32; 6] = [28, 31, 35, 33, 40, 26];

/// `BASS_GYM_FAKE_SESSION=grid` leaves the results closed, to show the beat grid.
pub fn opens_results() -> bool {
    std::env::var("BASS_GYM_FAKE_SESSION").as_deref() != Ok("grid")
}

/// `BASS_GYM_FAKE_SESSION=stats` opens the Statistics tab instead of Score + Audio.
pub fn opens_stats() -> bool {
    std::env::var("BASS_GYM_FAKE_SESSION").as_deref() == Ok("stats")
}

/// `base` shortened to a few bars, with a count-in.
pub fn config(base: &SessionConfig) -> SessionConfig {
    SessionConfig {
        count_in_bars: 1,
        auto_stop: AutoStop {
            enabled: true,
            when: StopCondition::Trigger(Trigger::After(Span::Bars(8))),
            ..Default::default()
        },
        ..base.clone()
    }
}

fn pluck(audio: &mut Vec<f32>, start: f64, seconds: f64, freq: f32) {
    let first = (start.max(0.0) * SR as f64) as usize;
    let len = (seconds * SR as f64) as usize;
    if audio.len() < first + len {
        audio.resize(first + len, 0.0);
    }
    for i in 0..len {
        let t = i as f32 / SR;
        let env = (t / 0.003).min(1.0) * (-t / 0.5).exp() * (1.0 - i as f32 / len as f32).sqrt();
        let tone: f32 = (1..5)
            .map(|h| (TAU * freq * h as f32 * t).sin() / h as f32)
            .sum();
        audio[first + i] += 0.3 * env * tone;
    }
}

/// Plays the session to its end and returns the recording.
pub fn run(logic: &mut dyn SessionLogic, shared: &SharedTiming, a4: f32) -> Recording {
    let mut sounds = Vec::new();
    let mut audio = Vec::new();
    let mut now = 0.0;
    logic.start(&mut SessionIo::offline_logged(now, 0.0, &mut sounds));
    let mut next = 0;
    let mut sounding: Option<(f64, f32)> = None;
    while logic.tick(&mut SessionIo::offline_logged(now, 0.0, &mut sounds)) == SessionState::Running
        && now < 120.0
    {
        if let Some((end, freq)) = sounding
            && now >= end
        {
            logic.note_off(end, -12.0, Some(freq));
            sounding = None;
        }
        let target = lock(shared)
            .matcher
            .expected
            .get(next)
            .map(|e| (e.time, e.len));
        if let Some((time, len)) = target {
            let at = time + ERRORS[next % ERRORS.len()];
            if now >= at {
                let n = next;
                next += 1;
                // Some bars get eighths, whatever the groove asks for.
                let eighths = (4..8).contains(&(n % 12));
                if n % 11 != 7 {
                    let freq = midi_to_freq(PITCHES[n % PITCHES.len()] as f32, a4);
                    let held = len * if eighths { 0.42 } else { 0.9 };
                    logic.note_on(at, -12.0);
                    logic.note_pitch(at, freq);
                    pluck(&mut audio, at, held + 0.03, freq);
                    if eighths {
                        logic.note_off(at + held, -12.0, Some(freq));
                    } else {
                        sounding = Some((at + held, freq));
                    }
                }
                if eighths {
                    let freq = midi_to_freq(PITCHES[(n + 2) % PITCHES.len()] as f32, a4);
                    let (at, held) = (time + len * 0.5 + 0.008, len * 0.42);
                    logic.note_on(at, -12.0);
                    logic.note_pitch(at, freq);
                    pluck(&mut audio, at, held + 0.03, freq);
                    logic.note_off(at + held, -12.0, Some(freq));
                }
            }
        }
        now += STEP;
    }
    logic.stop(&mut SessionIo::offline_logged(now, 0.0, &mut sounds));

    let mut recorder = Recorder::new(usize::MAX);
    audio.resize((now * SR as f64) as usize, 0.0);
    for (i, chunk) in audio.chunks(512).enumerate() {
        let time = i as f64 * 512.0 / SR as f64;
        recorder.feed(chunk, SR, 0, Some(time));
    }
    recorder.sounds_mut().extend(sounds);
    recorder.finish(now, 0.0)
}
