//! SoundFont (SF2) instruments, played with `rustysynth`. Fonts are found on disk at
//! startup and loaded on first use.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use crate::{SoundEvent, SoundKind, SoundRole, SoundSource, Voice};

/// Frames rendered between note-off checks; matches the output scheduler's block.
const BLOCK: usize = 16;
/// Output level, roughly matching the built-in voices.
const LEVEL: f32 = 3.0;

pub struct SoundFontSource {
    id: String,
    name: String,
    path: PathBuf,
    font: OnceLock<Result<Arc<SoundFont>, String>>,
}

impl SoundFontSource {
    /// A source for the SF2 file at `path`, named after its first preset (read lazily).
    pub fn new(path: &Path) -> Option<Self> {
        let stem = path.file_stem()?.to_str()?;
        let name = info_name(path).unwrap_or_else(|| stem.to_owned());
        Some(Self {
            id: format!("sf2:{stem}"),
            name,
            path: path.to_owned(),
            font: OnceLock::new(),
        })
    }

    fn font(&self) -> Result<Arc<SoundFont>, String> {
        self.font
            .get_or_init(|| {
                let started = std::time::Instant::now();
                let mut file = std::io::BufReader::new(
                    std::fs::File::open(&self.path).map_err(|e| e.to_string())?,
                );
                let font = SoundFont::new(&mut file).map_err(|e| format!("{e:?}"))?;
                log::info!(
                    "loaded {} in {:.0} ms",
                    self.path.display(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
                Ok(Arc::new(font))
            })
            .clone()
    }
}

impl SoundSource for SoundFontSource {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn roles(&self) -> &'static [SoundRole] {
        &[SoundRole::Pitched]
    }
    fn create(&self, sample_rate: f32) -> Box<dyn Voice> {
        match self
            .font()
            .and_then(|f| SoundFontVoice::new(&f, sample_rate))
        {
            Ok(v) => Box::new(v),
            Err(e) => {
                log::warn!(
                    "can't play {}: {e}; using the FM e-piano",
                    self.path.display()
                );
                Box::new(crate::FmPiano::new(sample_rate))
            }
        }
    }
}

/// The `INAM` (name) of a SoundFont, read from its header without loading the samples.
fn info_name(path: &Path) -> Option<String> {
    let mut head = vec![0u8; 4096];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    let head = &head[..n];
    if head.get(0..4)? != b"RIFF"
        || head.get(8..12)? != b"sfbk"
        || head.get(12..16)? != b"LIST"
        || head.get(20..24)? != b"INFO"
    {
        return None;
    }
    let info_end =
        (24 + u32::from_le_bytes(head.get(16..20)?.try_into().ok()?) as usize - 4).min(head.len());
    let mut pos = 24;
    while pos + 8 <= info_end {
        let id = &head[pos..pos + 4];
        let len = u32::from_le_bytes(head[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = head.get(pos + 8..(pos + 8 + len).min(head.len()))?;
        if id == b"INAM" {
            let s = String::from_utf8_lossy(body.split(|&b| b == 0).next()?)
                .trim()
                .to_owned();
            return (!s.is_empty()).then_some(s);
        }
        pos += 8 + len + len % 2;
    }
    None
}

/// All `.sf2` files in `dirs` (non-recursive), sorted by name.
pub fn find_soundfonts(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = dirs
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sf2")))
        .collect();
    found.sort_by_key(|p| p.file_name().map(|f| f.to_ascii_lowercase()));
    found
}

/// When each key's note-off is due, in frames: `held` until the note's written end,
/// then `tail` more for its release. A key ringing in its tail is cut at the next attack.
struct Holds {
    held: [u32; 128],
    tail: [u32; 128],
}

impl Holds {
    fn new() -> Self {
        Self {
            held: [0; 128],
            tail: [0; 128],
        }
    }

    fn start(&mut self, key: usize, held: u32, tail: u32) {
        self.held[key] = held.max(1);
        self.tail[key] = tail;
    }

    /// Ends every key ringing in its release, except `except` (about to be re-attacked).
    fn cut_tails(&mut self, except: usize, mut off: impl FnMut(usize)) {
        for key in 0..128 {
            if key != except && self.held[key] == 0 && self.tail[key] > 0 {
                self.tail[key] = 0;
                off(key);
            }
        }
    }

    /// Moves on `n` frames, calling `off` for each key whose note ends.
    fn advance(&mut self, n: u32, mut off: impl FnMut(usize)) {
        for key in 0..128 {
            let (h, t) = (&mut self.held[key], &mut self.tail[key]);
            if *h > 0 {
                if *h <= n {
                    *h = 0;
                    if *t == 0 {
                        off(key);
                    }
                } else {
                    *h -= n;
                }
            } else if *t > 0 {
                if *t <= n {
                    *t = 0;
                    off(key);
                } else {
                    *t -= n;
                }
            }
        }
    }
}

/// Plays the first preset of a SoundFont. Note lengths come with the trigger, so the
/// voice schedules its own note-offs, letting notes ring on for their `release`.
pub struct SoundFontVoice {
    synth: Synthesizer,
    left: [f32; BLOCK],
    right: [f32; BLOCK],
    holds: Holds,
    gain: f32,
}

impl SoundFontVoice {
    pub fn new(font: &Arc<SoundFont>, sample_rate: f32) -> Result<Self, String> {
        let mut settings = SynthesizerSettings::new(sample_rate.round() as i32);
        settings.block_size = BLOCK;
        settings.enable_reverb_and_chorus = false;
        let mut synth = Synthesizer::new(font, &settings).map_err(|e| format!("{e:?}"))?;
        if let Some(p) = font.get_presets().first() {
            synth.process_midi_message(0, 0xB0, 0x00, p.get_bank_number());
            synth.process_midi_message(0, 0xC0, p.get_patch_number(), 0);
        }
        Ok(Self {
            synth,
            left: [0.0; BLOCK],
            right: [0.0; BLOCK],
            holds: Holds::new(),
            gain: 1.0,
        })
    }
}

impl Voice for SoundFontVoice {
    fn trigger(&mut self, ev: &SoundEvent) {
        let (midi, velocity, duration, release) = match ev.kind {
            SoundKind::Note {
                midi,
                velocity,
                duration,
                release,
            } => (midi, velocity, duration, release),
            SoundKind::Click(_) => (72.0, 0.6, 0.1, 0.0),
        };
        let key = midi.round().clamp(0.0, 127.0) as usize;
        let synth = &mut self.synth;
        self.holds.cut_tails(key, |k| synth.note_off(0, k as i32));
        // The player scales by (velocity / 127)², on top of the recorded layer's own
        // level; keep MIDI velocities in the upper range so soft notes stay audible.
        let vel = (40.0 + 87.0 * velocity.clamp(0.0, 1.0)).round() as i32;
        self.gain = ev.gain;
        self.synth.note_on(0, key as i32, vel);
        let sr = self.synth.get_sample_rate() as f32;
        self.holds.start(
            key,
            (duration.max(0.03) * sr) as u32,
            (release.max(0.0) * sr) as u32,
        );
    }

    fn render(&mut self, out: &mut [f32]) {
        for chunk in out.chunks_mut(BLOCK) {
            let synth = &mut self.synth;
            self.holds
                .advance(chunk.len() as u32, |k| synth.note_off(0, k as i32));
            let (l, r) = (
                &mut self.left[..chunk.len()],
                &mut self.right[..chunk.len()],
            );
            self.synth.render(l, r);
            for ((o, a), b) in chunk.iter_mut().zip(l.iter()).zip(r.iter()) {
                *o += (a + b) * 0.5 * LEVEL * self.gain;
            }
        }
    }

    fn silence(&mut self) {
        self.synth.note_off_all(true);
        self.holds = Holds::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offs(h: &mut Holds, n: u32) -> Vec<usize> {
        let mut out = Vec::new();
        h.advance(n, |k| out.push(k));
        out
    }

    #[test]
    fn notes_end_after_their_length_and_release() {
        let mut h = Holds::new();
        h.start(60, 100, 0);
        h.start(64, 100, 50);
        assert!(offs(&mut h, 99).is_empty());
        assert_eq!(offs(&mut h, 1), [60]);
        assert!(offs(&mut h, 49).is_empty());
        assert_eq!(offs(&mut h, 1), [64]);
        assert!(offs(&mut h, 1000).is_empty());
    }

    #[test]
    fn the_next_attack_cuts_ringing_notes_but_not_held_ones() {
        let mut h = Holds::new();
        h.start(60, 10, 1000);
        h.start(64, 10, 1000);
        offs(&mut h, 10);
        h.start(67, 500, 0);
        let mut cut = Vec::new();
        // 64 is re-attacked: only 60 is cut, and the held 67 isn't touched.
        h.cut_tails(64, |k| cut.push(k));
        assert_eq!(cut, [60]);
        cut.clear();
        h.cut_tails(72, |k| cut.push(k));
        assert_eq!(cut, [64]);
    }
}
