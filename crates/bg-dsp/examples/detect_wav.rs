//! Runs a recording through the note detector and prints what it finds, to check
//! detection against an exported report:
//!
//! `cargo run -p bg-dsp --example detect_wav -- <audio.wav> [release_db]`

use bg_dsp::{NoteDetector, NoteDetectorConfig, NoteEvent};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: detect_wav <audio.wav> [release_db]");
    let mut cfg = NoteDetectorConfig::default();
    if let Some(release) = args.next() {
        cfg.release_db = release.parse().expect("release_db is a number");
    }
    let mut wav = hound::WavReader::open(path).expect("readable WAV file");
    let sr = wav.spec().sample_rate as f32;
    let samples: Vec<f32> = match wav.spec().sample_format {
        hound::SampleFormat::Float => wav
            .samples::<f32>()
            .map(|s| s.expect("readable sample"))
            .collect(),
        hound::SampleFormat::Int => wav
            .samples::<i16>()
            .map(|s| s.expect("16-bit WAV") as f32 / 32768.0)
            .collect(),
    };

    let mut detector = NoteDetector::new(sr, cfg);
    let mut events = Vec::new();
    for chunk in samples.chunks(256) {
        detector.process(chunk, &mut events);
    }
    for e in events {
        match e {
            NoteEvent::On { sample, level_db } => {
                println!("{:8.3} on   {level_db:6.1} dB", sample as f32 / sr);
            }
            NoteEvent::Off { sample, peak_db } => {
                println!("{:8.3} off  {peak_db:6.1} dB", sample as f32 / sr);
            }
            NoteEvent::Noise {
                sample,
                end,
                peak_db,
            } => {
                println!(
                    "{:8.3} noise {peak_db:5.1} dB, to {:.3}",
                    sample as f32 / sr,
                    end as f32 / sr
                );
            }
        }
    }
}
