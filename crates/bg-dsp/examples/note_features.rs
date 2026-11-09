//! Prints measurements of every note the detector finds in a recording, to tell real
//! notes, ghost notes and string noise apart:
//!
//! `cargo run -p bg-dsp --example note_features -- <audio.wav>`

use bg_dsp::filter::Biquad;
use bg_dsp::{NoteDetector, NoteDetectorConfig, NoteEvent, db, detect};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: note_features <audio.wav>");
    let mut wav = hound::WavReader::open(path).expect("readable WAV file");
    let sr = wav.spec().sample_rate as f32;
    let samples: Vec<f32> = match wav.spec().sample_format {
        hound::SampleFormat::Float => wav.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => wav
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect(),
    };
    let mut detector = NoteDetector::new(sr, NoteDetectorConfig::default());
    let mut events = Vec::new();
    for chunk in samples.chunks(256) {
        detector.process(chunk, &mut events);
    }
    let mut hp = [
        Biquad::highpass(sr, 500.0, 0.54),
        Biquad::highpass(sr, 500.0, 1.31),
    ];
    let hf: Vec<f32> = samples
        .iter()
        .map(|&x| {
            let y = hp[0].process(x);
            hp[1].process(y)
        })
        .collect();

    let mut notes = Vec::new();
    let mut on = None;
    for e in &events {
        match *e {
            NoteEvent::On { sample, .. } => on = Some(sample as usize),
            NoteEvent::Off { sample, .. } => {
                if let Some(s) = on.take() {
                    notes.push((s, sample as usize, ""));
                }
            }
            NoteEvent::Noise { sample, end, .. } => {
                notes.push((sample as usize, end as usize, "noise"));
            }
        }
    }
    if let Some(s) = on {
        notes.push((s, samples.len(), ""));
    }
    let ms = |n: usize| (n as f32 / sr * 1000.0) as usize;
    let at = |ms: f32| (ms / 1000.0 * sr) as usize;
    let peak = |a: &[f32]| a.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let energy = |a: &[f32]| a.iter().map(|v| v * v).sum::<f32>();
    println!("   time   dur  peak  pk10  pk30  30-60  hf30  pre50    freq clar");
    notes.sort_by_key(|n| n.0);
    for (s, e, kind) in notes {
        let e = e.min(samples.len());
        let n = &samples[s..e];
        let w = |a: usize, b: usize| &samples[(s + a).min(e)..(s + b).min(e)];
        let first30 = w(0, at(30.0));
        let hf30 = &hf[s..(s + at(30.0)).min(e)];
        let pre = &samples[s.saturating_sub(at(50.0))..s];
        let reading = detect(&samples[(s + at(8.0)).min(e)..e.min(s + at(600.0))], sr);
        println!(
            "{:7.3} {:5} {:5.1} {:5.1} {:5.1} {:6.1} {:5.1} {:6.1} {:7} {:4} {kind}",
            s as f32 / sr,
            ms(e - s),
            db(peak(n)),
            db(peak(w(0, at(10.0)))),
            db(peak(first30)),
            db(peak(w(at(30.0), at(60.0)))) - db(peak(first30)),
            10.0 * (energy(hf30) / energy(first30).max(1e-12)).log10(),
            db(peak(pre)),
            reading.map_or("-".into(), |r| format!("{:.1}", r.freq)),
            reading.map_or("-".into(), |r| format!("{:.2}", r.clarity)),
        );
    }
}
