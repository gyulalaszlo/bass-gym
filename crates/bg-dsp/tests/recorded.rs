//! Note detection on clips of real playing, cut from exported reports.

use bg_dsp::{NoteDetector, NoteDetectorConfig, NoteEvent};

/// Onset times in seconds of a clip in `tests/fixtures`, leaving out the very start,
/// where a clip cut from a session can begin inside the previous note.
fn onsets(name: &str) -> Vec<f32> {
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
    events
        .iter()
        .filter_map(|e| match e {
            NoteEvent::On { sample, .. } => Some(*sample as f32 / sr),
            _ => None,
        })
        .filter(|&t| t > 0.02)
        .collect()
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
        0.076, 0.424, 0.788, 1.137, 1.502, 1.867, 2.241, 2.591, 2.969,
    ];
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (f, e) in found.iter().zip(expected) {
        assert!((f - e).abs() < 0.003, "{found:?}");
    }
}
