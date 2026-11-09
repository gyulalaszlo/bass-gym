//! Basic chords end to end, without audio hardware: the piano accompaniment plays a
//! ii–V–I, a synthesized bassist plays chord tones, a passing tone, a clash and a late
//! note, and the session grades them through the real note and pitch detection.

use std::f32::consts::TAU;

use bg_audio::{SessionIo, SessionLogic, SessionState};
use bg_core::harmony::{self, Scale};
use bg_core::preset::{PresetSchema, migrate};
use bg_core::session::{AutoStop, Span, StopCondition, Trigger};
use bg_core::{Grade, GradeMode, Planner, ScoringThresholds, SessionConfig};
use bg_dsp::{NoteDetector, NoteDetectorConfig, NoteEvent};
use bg_exercises::basic_chords::accompaniment::{ChordAccompaniment, SharedChords, lock_chords};
use bg_exercises::basic_chords::{
    BasicChords, BasicChordsData, ChordLength, FixedSource, ProgressionKind, resolve_progression,
};
use bg_exercises::timing_session::{
    Accompaniment, AccompanimentScoring, SharedTiming, TimingSession, lock,
};
use bg_ui::ExerciseType;
use serde_json::json;

const SR: f32 = 48_000.0;

fn pluck(out: &mut [f32], start: f64, dur: f64, midi: i32) {
    let freq = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
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

fn session_cfg() -> SessionConfig {
    SessionConfig {
        tempo: 120.0,
        count_in_bars: 1,
        auto_stop: AutoStop {
            enabled: true,
            when: StopCondition::Trigger(Trigger::After(Span::Bars(4))),
            ..Default::default()
        },
        ..Default::default() // quarter-note groove
    }
}

/// The bass line of `run`: (midi, timing error ms) per groove beat.
const BASS: [(i32, f64); 12] = [
    // Dm7: chord tones
    (38, 0.0),
    (41, 3.0),
    (45, -4.0),
    (48, 2.0),
    // G7: G, B, E (passing), C# (clash)
    (43, 0.0),
    (47, 0.0),
    (40, 0.0),
    (37, 0.0),
    // Cmaj7: C, E 25 ms late, G, B
    (36, 0.0),
    (40, 25.0),
    (43, 0.0),
    (47, 0.0),
];

fn run(mode: GradeMode) -> (SharedTiming, SharedChords) {
    run_with(
        AccompanimentScoring {
            grade_mode: mode,
            ..Default::default()
        },
        session_cfg(),
        0.4,
    )
}

/// Plays `BASS` (each note `dur` seconds) along a ii–V–I in half notes.
fn run_with(
    scoring: AccompanimentScoring,
    cfg: SessionConfig,
    dur: f64,
) -> (SharedTiming, SharedChords) {
    let scale = Scale {
        root: 0,
        mode: harmony::Mode::Ionian,
    };
    let specs = harmony::parse_progression("ii7 V7 I7 I7", &scale).unwrap();
    let progression: Vec<_> = specs.iter().map(|&s| scale.realize(s)).collect();
    let chords = SharedChords::default();
    let acc = ChordAccompaniment::new(
        progression,
        scale,
        harmony::checker("chord_or_scale"),
        "halves".into(),
        1.0,
        harmony::VoicingStyle {
            jazziness: 0.3,
            ..Default::default()
        },
        0.7,
        440.0,
        7,
        chords.clone(),
    );
    let timing = SharedTiming::default();
    let mut session = TimingSession::with_accompaniment(
        cfg.clone(),
        ScoringThresholds::default(),
        timing.clone(),
        Box::new(acc),
        scoring,
    );

    let mut planner = Planner::new(cfg, 0.3);
    let expected: Vec<f64> = std::iter::from_fn(|| planner.next_bar(&()))
        .flat_map(|b| b.notes)
        .map(|n| n.time)
        .collect();
    assert_eq!(expected.len(), 16);
    let total = expected.last().unwrap() + 1.5;
    let mut audio = vec![0.0f32; (total * SR as f64) as usize];
    for (&t, &(midi, err)) in expected.iter().zip(&BASS) {
        pluck(&mut audio, t + err / 1000.0, dur, midi);
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
                    // Like the audio worker: the pitch from the note's own samples.
                    let from = onset as usize + (0.008 * SR) as usize;
                    let freq = bg_dsp::detect(&audio[from..sample as usize], SR).map(|r| r.freq);
                    if let Some(f) = freq {
                        session.note_pitch(onset as f64 / SR as f64, f);
                    }
                    session.note_off(sample as f64 / SR as f64, peak_db, freq);
                }
            }
        }
        let now = ((i + 1) * 256) as f64 / SR as f64;
        if session.tick(&mut SessionIo::offline(now, 0.0)) == SessionState::Finished {
            session.stop(&mut SessionIo::offline(now, 0.0));
            break;
        }
    }
    (timing, chords)
}

#[test]
fn chord_session_grades_harmony() {
    use Grade::*;
    let (timing, chords) = run(GradeMode::Combined);
    let t = lock(&timing);
    let grades: Vec<Grade> = t
        .matcher
        .outcomes
        .iter()
        .map(|o| o.grade.unwrap())
        .collect();
    assert_eq!(
        grades,
        [
            Good, Good, Good, Good, Good, Good, Ok, Poor, Good, Ok, Good, Good, Missed, Missed,
            Missed, Missed
        ]
    );
    let stats = t.matcher.stats();
    assert_eq!(stats.harmony, [10, 1, 1, 0]);

    // The piano played four chords, one per bar, with smooth voice leading.
    let c = lock_chords(&chords);
    let names: Vec<String> = c.spans.iter().map(|s| s.chord.name()).collect();
    assert_eq!(names, ["Dm7", "G7", "Cmaj7", "Cmaj7"]);
    for s in &c.spans {
        assert!(
            c.piano.iter().any(|n| (n.time - s.start).abs() < 1e-6),
            "no attack at {}",
            s.start
        );
        assert!((s.end - s.start - 2.0).abs() < 1e-6);
    }
    let top = |v: &Vec<u8>| *v.last().unwrap() as i32;
    for w in c.spans.windows(2) {
        assert!(
            (top(&w[0].voicing) - top(&w[1].voicing)).abs() <= 5,
            "{:?} -> {:?}",
            w[0].voicing,
            w[1].voicing
        );
    }

    let (timing, _) = run(GradeMode::Separate);
    let t = lock(&timing);
    let grades: Vec<Grade> = t
        .matcher
        .outcomes
        .iter()
        .map(|o| o.grade.unwrap())
        .collect();
    assert_eq!(grades[4..8], [Good, Good, Good, Good]);
    assert_eq!(t.matcher.outcomes[7].harmony_grade, Some(Poor));
    assert_eq!(t.matcher.outcomes[9].grade, Some(Ok));
}

#[test]
fn piano_hits_as_targets() {
    // Half-note comp ("halves"): the piano attacks beats 1 and 3. With only piano targets,
    // the bass notes on beats 2 and 4 are extras, still judged for harmony.
    let (timing, chords) = run_with(
        AccompanimentScoring {
            groove: false,
            hits: true,
            ..Default::default()
        },
        session_cfg(),
        0.4,
    );
    let t = lock(&timing);
    let m = &t.matcher;
    let c = lock_chords(&chords);
    let starts: Vec<f64> = m.expected.iter().map(|e| e.time).collect();
    assert_eq!(starts.len(), 8, "{starts:?}");
    for w in starts.windows(2) {
        assert!((w[1] - w[0] - 1.0).abs() < 1e-6, "{starts:?}");
    }
    assert!(
        c.spans
            .iter()
            .all(|s| starts.iter().any(|&t| (t - s.start).abs() < 1e-6))
    );
    let hits = m.outcomes[..6]
        .iter()
        .filter(|o| o.played.is_some())
        .count();
    assert_eq!(hits, 6);
    let s = m.stats();
    assert_eq!(s.extra, 6);
    // Harmony covers all 12 notes, extras included.
    assert_eq!(s.harmony, [10, 1, 1, 0]);
}

#[test]
fn free_play_grades_only_harmony() {
    let (timing, _) = run_with(
        AccompanimentScoring {
            groove: false,
            ..Default::default()
        },
        session_cfg(),
        0.4,
    );
    let t = lock(&timing);
    assert!(t.matcher.expected.is_empty());
    let s = t.matcher.stats();
    assert_eq!((s.total(), s.extra), (0, 12));
    assert_eq!(s.harmony, [10, 1, 1, 0]);
}

#[test]
fn harmony_off_grades_timing_only() {
    use Grade::*;
    let (timing, _) = run_with(
        AccompanimentScoring {
            harmony: false,
            ..Default::default()
        },
        session_cfg(),
        0.4,
    );
    let t = lock(&timing);
    let m = &t.matcher;
    assert!(m.outcomes.iter().all(|o| o.harmony.is_none()));
    assert!(
        m.played
            .iter()
            .all(|p| p.harmony.is_none() && p.freq.is_some())
    );
    // The clash (beat 8) is Good now; only the late note is Ok.
    let grades: Vec<Grade> = m.outcomes[..12].iter().map(|o| o.grade.unwrap()).collect();
    assert_eq!(
        grades,
        [
            Good, Good, Good, Good, Good, Good, Good, Good, Good, Ok, Good, Good
        ]
    );
    assert_eq!(m.stats().harmony, [0; 4]);
}

#[test]
fn note_length_scoring_follows_the_preset() {
    // Staccato notes: an eighth of a beat against quarter-note targets.
    let grade = |score_length| {
        let cfg = SessionConfig {
            score_length,
            ..session_cfg()
        };
        let (timing, _) = run_with(
            AccompanimentScoring {
                harmony: false,
                ..Default::default()
            },
            cfg,
            0.12,
        );
        lock(&timing).matcher.outcomes[0].grade.unwrap()
    };
    assert_eq!(grade(false), Grade::Good);
    assert_eq!(grade(true), Grade::Poor);
}

#[test]
fn anticipated_chords_start_their_span_early() {
    // A jazzy player pushes some chord changes an eighth early; the spans (and so the
    // progression strip) must change there, not at the bar line.
    let scale = Scale {
        root: 0,
        mode: harmony::Mode::Ionian,
    };
    let specs = harmony::parse_progression("ii7 V7 I7 vi7", &scale).unwrap();
    let progression: Vec<_> = specs.iter().map(|&s| scale.realize(s)).collect();
    let chords = SharedChords::default();
    let mut acc = ChordAccompaniment::new(
        progression,
        scale,
        harmony::checker("chord_tones"),
        "halves".into(),
        1.0,
        harmony::VoicingStyle {
            jazziness: 1.0,
            ..Default::default()
        },
        0.7,
        440.0,
        3,
        chords.clone(),
    );
    let cfg = SessionConfig {
        count_in_bars: 0,
        ..session_cfg()
    };
    let mut planner = Planner::new(cfg, 1.0);
    let bars: Vec<_> = std::iter::from_fn(|| planner.next_bar(&()))
        .take(16)
        .collect();
    for b in &bars {
        acc.plan_bar(b, &mut SessionIo::offline(0.0, 0.0));
    }
    let c = lock_chords(&chords);
    let beat = bars[0].duration / bars[0].beats as f64;
    let pushed = c
        .spans
        .iter()
        .filter(|s| {
            bars.iter()
                .any(|b| (b.start - 0.5 * beat - s.start).abs() < 1e-6)
        })
        .count();
    assert!(pushed > 0, "no anticipations in 16 bars");
    for w in c.spans.windows(2) {
        assert!((w[0].end - w[1].start).abs() < 1e-6, "gap or overlap");
        assert_ne!(w[0].step, w[1].step);
    }
    // Every span starts with the piano playing its voicing.
    for s in &c.spans {
        let attack: Vec<u8> = c
            .piano
            .iter()
            .filter(|n| (n.time - s.start).abs() < 1e-6)
            .map(|n| n.midi)
            .collect();
        assert_eq!(attack, s.voicing, "at {}", s.start);
    }
}

#[test]
fn v1_presets_migrate() {
    let et = BasicChords::default();
    let v1 = json!({
        "session": { "tempo": 100.0 },
        "scale": { "root": 9, "mode": "Aeolian" },
        "sevenths": true,
        "progression_len": 6,
        "bars_per_chord": 2,
        "seed": 42,
        "checker": "chord_or_scale"
    });
    let data = migrate(&et, 1, v1).unwrap();
    let d: BasicChordsData = serde_json::from_value(data.clone()).unwrap();
    assert_eq!(d.progression.kind, ProgressionKind::Random);
    assert_eq!(d.progression.random_len, 6);
    assert_eq!(d.chord_length, ChordLength::Two);
    assert!((d.jazziness - 0.35).abs() < 1e-6);
    assert_eq!(
        (d.seed, d.checker.as_str(), d.session.tempo),
        (42, "chord_or_scale", 100.0)
    );
    // v2 moved the key into the session settings.
    assert_eq!(
        d.session.key,
        Scale {
            root: 9,
            mode: harmony::Mode::Aeolian
        }
    );
    let mut et = et;
    et.load_preset(data).unwrap();
    assert_eq!(et.current_version(), 3);
}

#[test]
fn progressions_resolve_in_every_mode() {
    let mut d = BasicChordsData::default();
    for kind in [
        ProgressionKind::Fixed,
        ProgressionKind::SemiRandom,
        ProgressionKind::Random,
    ] {
        d.progression.kind = kind;
        let a = resolve_progression(&d, 5).unwrap();
        assert!(!a.is_empty());
        assert_eq!(a, resolve_progression(&d, 5).unwrap(), "{kind:?} is seeded");
    }
    d.progression.kind = ProgressionKind::Fixed;
    d.progression.source = FixedSource::Custom;
    d.progression.custom = "ii7 V7 Q".into();
    assert!(resolve_progression(&d, 1).is_err());
    d.progression.kind = ProgressionKind::SemiRandom;
    d.progression.pool = vec!["ii_v_i".into()];
    let names: Vec<String> = resolve_progression(&d, 3)
        .unwrap()
        .iter()
        .map(|c| c.roman())
        .collect();
    assert_eq!(&names[..2], ["ii7", "V7"]);
}
