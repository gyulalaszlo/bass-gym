//! Packs multi-sampled recordings into SoundFonts, one per set:
//!
//! ```text
//! cargo run -p sf2pack --release -- samples/e-piano assets/soundfonts --name E-Piano
//! ```
//!
//! Every subdirectory of the input is a set (e.g. `Mellow/`) of FLAC files named
//! `<anything>vel<velocity>-<note><octave>.flac` (e.g. `Mellow--vel064-A4.flac`, C4 = 60).
//! Silence before the attack is cut (so notes sound on time), tails are trimmed where
//! they fade into the noise floor, and each set is normalized as a whole, keeping the
//! levels of the velocity layers.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sf2pack::{Instrument, Sample, Zone, key_ranges};

struct Recording {
    path: PathBuf,
    root: u8,
    velocity: u8,
    data: Vec<f32>,
    rate: u32,
}

/// Parses `...vel064-Bb1.flac` into (velocity, MIDI note).
fn parse_name(name: &str) -> Option<(u8, u8)> {
    let stem = name.strip_suffix(".flac").or_else(|| name.strip_suffix(".FLAC"))?;
    let (_, rest) = stem.rsplit_once("vel")?;
    let (vel, note) = rest.split_once('-')?;
    let vel: u8 = vel.parse().ok()?;
    let mut chars = note.chars();
    let letter = chars.next()?;
    let mut pc: i32 = match letter.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let octave = if let Some(r) = rest.strip_prefix('b') {
        pc -= 1;
        r
    } else if let Some(r) = rest.strip_prefix('#') {
        pc += 1;
        r
    } else {
        rest
    };
    let octave: i32 = octave.parse().ok()?;
    let midi = (octave + 1) * 12 + pc;
    (0..=127).contains(&midi).then_some((vel.min(127), midi as u8))
}

fn read_flac(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut r = claxon::FlacReader::open(path).with_context(|| format!("reading {}", path.display()))?;
    let info = r.streaminfo();
    let ch = info.channels.max(1) as usize;
    let scale = 1.0 / (1u64 << (info.bits_per_sample - 1)) as f32;
    let raw: Vec<f32> = r.samples().map(|s| s.map(|s| s as f32 * scale)).collect::<Result<_, _>>().with_context(|| format!("decoding {}", path.display()))?;
    // Mix down to mono.
    Ok((raw.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect(), info.sample_rate))
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Cuts leading silence and the tail below the noise floor, then fades out.
fn trim(x: &[f32], rate: u32, floor: f32, max_seconds: f32) -> Vec<f32> {
    let own = peak(x);
    let onset = own * 0.02; // −34 dB
    let lead = (rate / 1000) as usize; // keep 1 ms before the attack
    let start = x.iter().position(|s| s.abs() > onset).unwrap_or(0).saturating_sub(lead);
    let win = (rate / 100) as usize;
    let threshold = (own * 1e-3).max(floor); // −60 dB, or the set's noise floor
    let last_loud = x[start..].chunks(win).rposition(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt() > threshold).map_or(0, |i| (i + 1) * win);
    let len = last_loud.min((max_seconds * rate as f32) as usize).min(x.len() - start);
    let mut out = x[start..start + len].to_vec();
    let fade = (len / 5).min((rate as f32 * 0.4) as usize).max(1);
    for (i, s) in out[len - fade..].iter_mut().enumerate() {
        let g = 1.0 - i as f32 / fade as f32;
        *s *= g * g;
    }
    out
}

/// 16-bit with triangular dither.
fn quantize(x: &[f32], gain: f32, seed: &mut u32) -> Vec<i16> {
    let mut rnd = || {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 17;
        *seed ^= *seed << 5;
        *seed as f32 / u32::MAX as f32
    };
    x.iter().map(|&s| (s * gain * 32_767.0 + rnd() - rnd()).round().clamp(-32_768.0, 32_767.0) as i16).collect()
}

fn pack_set(dir: &Path, name: &str, max_seconds: f32) -> Result<Instrument> {
    let mut recs = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(file) = path.file_name().and_then(|f| f.to_str()) else { continue };
        let Some((velocity, root)) = parse_name(file) else {
            if file.to_ascii_lowercase().ends_with(".flac") {
                eprintln!("  skipping {file}: expected <name>vel<velocity>-<note><octave>.flac");
            }
            continue;
        };
        let (data, rate) = read_flac(&path)?;
        recs.push(Recording { path, root, velocity, data, rate });
    }
    if recs.is_empty() {
        bail!("no samples in {}", dir.display());
    }
    recs.sort_by_key(|r| (r.root, r.velocity));

    let set_peak = recs.iter().map(|r| peak(&r.data)).fold(0.0, f32::max);
    // Noise floor: the quietest stretch at the end of any recording.
    let floor = recs
        .iter()
        .filter_map(|r| {
            let tail = &r.data[r.data.len().saturating_sub((r.rate / 10) as usize)..];
            (!tail.is_empty()).then(|| (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt())
        })
        .fold(f32::MAX, f32::min);
    let floor = (floor * 2.0).max(set_peak * 10f32.powf(-80.0 / 20.0));
    println!("{name}: {} samples, peak {:.1} dBFS, noise floor {:.1} dB", recs.len(), db(set_peak), db(floor));

    let gain = 10f32.powf(-1.0 / 20.0) / set_peak;
    let mut seed = 0x9E37_79B9;
    let mut samples = Vec::new();
    let (mut before, mut after) = (0usize, 0usize);
    for r in &recs {
        let t = trim(&r.data, r.rate, floor, max_seconds);
        before += r.data.len();
        after += t.len();
        let label = r.path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        samples.push(Sample { name: label.rsplit("--").next().unwrap_or(label).to_owned(), rate: r.rate, root: r.root, data: quantize(&t, gain, &mut seed) });
    }
    println!("  kept {:.1} of {:.1} seconds of audio", after as f32 / recs[0].rate as f32, before as f32 / recs[0].rate as f32);

    // Keys: split between the recorded notes. Velocities: split halfway between layers.
    let mut roots: Vec<u8> = recs.iter().map(|r| r.root).collect();
    roots.sort_unstable();
    roots.dedup();
    let ranges = key_ranges(&roots);
    let mut zones = Vec::new();
    for (root, keys) in roots.iter().zip(ranges) {
        let layers: Vec<(usize, u8)> = recs.iter().enumerate().filter(|(_, r)| r.root == *root).map(|(i, r)| (i, r.velocity)).collect();
        for (j, &(sample, vel)) in layers.iter().enumerate() {
            let lo = if j == 0 { 0 } else { ((layers[j - 1].1 as u16 + vel as u16) / 2) as u8 + 1 };
            let hi = layers.get(j + 1).map_or(127, |n| ((vel as u16 + n.1 as u16) / 2) as u8);
            zones.push(Zone { sample, keys, vels: (lo, hi) });
        }
    }
    Ok(Instrument { name: name.to_owned(), samples, zones, release: 0.25 })
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut positional = Vec::new();
    let mut prefix = None;
    let mut max_seconds = 12.0;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--name" => prefix = it.next().cloned(),
            "--max-seconds" => max_seconds = it.next().context("--max-seconds needs a value")?.parse()?,
            _ => positional.push(a.clone()),
        }
    }
    let [input, output] = positional.as_slice() else {
        bail!("usage: sf2pack <input dir> <output dir> [--name <instrument name prefix>] [--max-seconds <s>]");
    };
    let (input, output) = (PathBuf::from(input), PathBuf::from(output));
    std::fs::create_dir_all(&output)?;
    let mut sets: Vec<PathBuf> = std::fs::read_dir(&input)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect();
    sets.sort();
    if sets.is_empty() {
        bail!("no sample sets (subdirectories) in {}", input.display());
    }
    for set in sets {
        let set_name = set.file_name().and_then(|s| s.to_str()).unwrap_or("set");
        let name = prefix.as_ref().map_or_else(|| set_name.to_owned(), |p| format!("{p} {set_name}"));
        let inst = pack_set(&set, &name, max_seconds)?;
        let path = output.join(format!("{}.sf2", slug(&name)));
        let mut file = std::io::BufWriter::new(std::fs::File::create(&path)?);
        sf2pack::write(&mut file, &inst)?;
        drop(file);
        println!("  wrote {} ({:.1} MB)", path.display(), std::fs::metadata(&path)?.len() as f64 / 1e6);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_names() {
        assert_eq!(parse_name("Mellow--vel064-A4.flac"), Some((64, 69)));
        assert_eq!(parse_name("Crunchy--vel001-Eb1.flac"), Some((1, 27)));
        assert_eq!(parse_name("Dirty--vel127-Db7.flac"), Some((127, 97)));
        assert_eq!(parse_name("x--vel016-C3.flac"), Some((16, 48)));
        assert_eq!(parse_name("Mellow--vel064-A4.wav"), None);
    }

    #[test]
    fn trims_silence_and_tail() {
        let rate = 1000;
        let mut x = vec![0.0; 100];
        x.extend((0..2000).map(|i| (-(i as f32) / 200.0).exp() * if i % 2 == 0 { 1.0 } else { -1.0 }));
        let t = trim(&x, rate, 1e-4, 12.0);
        assert!(t[0].abs() < 1e-6 && t[1].abs() > 0.5, "attack within 1 ms");
        assert!(t.len() < 1500, "tail trimmed: {}", t.len());
        assert!(t.last().unwrap().abs() < 1e-3);
    }
}
