//! Note detection on clips of real playing, cut from exported reports.

use bg_dsp::{NoteDetector, NoteDetectorConfig, NoteEvent};

/// What the detector reports for a clip in `tests/fixtures`, with its sample rate.
fn events(name: &str) -> (Vec<NoteEvent>, f32) {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let mut wav = hound::WavReader::open(path).unwrap();
    let sr = wav.spec().sample_rate as f32;
    let samples: Vec<f32> = wav
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / 32768.0)
        .collect();
    let mut detector = NoteDetector::new(sr, NoteDetectorConfig::default());
    let mut events = Vec::new();
    for chunk in samples.chunks(256) {
        detector.process(chunk, &mut events);
    }
    (events, sr)
}

/// Onset times in seconds of a clip in `tests/fixtures`, leaving out the very start,
/// where a clip cut from a session can begin inside the previous note.
fn onsets(name: &str) -> Vec<f32> {
    let (events, sr) = events(name);
    events
        .iter()
        .filter_map(|e| match e {
            NoteEvent::On { sample, .. } => Some(*sample as f32 / sr),
            _ => None,
        })
        .filter(|&t| t > 0.02)
        .collect()
}

/// Start times of what was reported as string noise.
fn noise(name: &str) -> Vec<f32> {
    let (events, sr) = events(name);
    events
        .iter()
        .filter_map(|e| match e {
            NoteEvent::Noise { sample, .. } => Some(*sample as f32 / sr),
            _ => None,
        })
        .collect()
}

fn assert_near(found: &[f32], expected: &[f32], tolerance: f32) {
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (f, e) in found.iter().zip(expected) {
        assert!((f - e).abs() < tolerance, "{found:?}");
    }
}

/// Quarter notes with string noise between some of them: the touch of a finger before a
/// note, or lifting off after one. Noise peaks 15-25 dB below the notes.
#[test]
fn string_noise_is_not_a_note() {
    let name = "string_noise.wav";
    assert_near(&noise(name), &[2.525, 4.453, 5.102], 0.005);
    // The note at 2.63 starts 22 dB down for 9 ms before its pluck: it starts there.
    assert_near(
        &onsets(name),
        &[0.070, 0.717, 1.363, 2.632, 3.258, 3.879, 4.533, 5.163],
        0.004,
    );
}

/// Muted plucks (ghost notes), a real note and string noise. Most ghost notes start with
/// the faint touch of the finger, their thud following 15-30 ms later: the thud is the
/// onset (0.45, 1.36, 1.79, 2.70, 3.56).
#[test]
fn ghost_notes_start_at_their_thud() {
    let name = "ghost_notes.wav";
    assert_near(&noise(name), &[0.746, 2.624, 3.967], 0.005);
    assert_near(
        &onsets(name),
        &[
            0.164, 0.453, 0.934, 1.363, 1.788, 2.254, 2.702, 3.146, 3.560,
        ],
        0.004,
    );
}

/// One low E, whose waveform has a sharp edge in every period. It was detected as three
/// notes, the edges passing for plucks.
#[test]
fn a_ringing_low_e_is_one_note() {
    let found = onsets("low_e_single_note.wav");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!((found[0] - 0.075).abs() < 0.01, "{found:?}");
}

/// Eighth notes on one pitch, some with almost no gap between them: all must be found.
#[test]
fn repeated_notes_are_all_found() {
    let found = onsets("repeated_eighths.wav");
    let expected = [
        0.076, 0.424, 0.788, 1.137, 1.502, 1.867, 2.238, 2.591, 2.969,
    ];
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (f, e) in found.iter().zip(expected) {
        assert!((f - e).abs() < 0.003, "{found:?}");
    }
}

/// Eighths on one pitch at 140 BPM, each muted only 30-50 ms before the next: too briefly
/// for a window sized for the lowest note to see. Every second note went missing.
#[test]
fn fast_eighths_are_all_found() {
    let found = onsets("fast_eighths.wav");
    let expected = [
        0.046, 0.223, 0.479, 0.716, 0.932, 1.164, 1.368, 1.582, 1.785, 1.982, 2.184, 2.392, 2.602,
    ];
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (f, e) in found.iter().zip(expected) {
        assert!((f - e).abs() < 0.005, "{found:?}");
    }
}

/// Half notes; the mute at the end of the second makes a sound of its own, only 9 dB
/// below the notes and 62 ms before the next one, 10 dB louder: noise.
#[test]
fn the_sound_of_a_mute_before_a_note_is_noise() {
    let name = "mute_noise.wav";
    assert_near(&noise(name), &[2.039], 0.005);
    assert_near(&onsets(name), &[0.489, 1.289, 2.101, 3.700], 0.004);
}
