//! Sound generation. Every sound comes from a [`Voice`] created by a registered
//! [`SoundSource`]: built-in synthesized voices, plus SoundFont instruments found on disk
//! ([`register_soundfonts`]).

mod soundfont;
mod synth;

use std::path::PathBuf;
use std::sync::RwLock;

use bg_core::plan::ClickKind;

pub use soundfont::{SoundFontSource, SoundFontVoice, find_soundfonts};
pub use synth::{FmPiano, PluckBass, SineClick, Woodblock};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundKind {
    Click(ClickKind),
    /// `duration` in seconds; `midi` may be fractional.
    Note { midi: f32, velocity: f32, duration: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundEvent {
    pub kind: SoundKind,
    pub gain: f32,
}

/// A real-time sound generator. `trigger` and `render` run on the audio thread and must
/// not allocate or block.
pub trait Voice: Send {
    fn trigger(&mut self, ev: &SoundEvent);
    /// Adds (mixes) mono output into `out`.
    fn render(&mut self, out: &mut [f32]);
    fn silence(&mut self);
}

/// What a sound source is meant for; used to filter choices in the preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundRole {
    Click,
    Pitched,
}

pub trait SoundSource: Sync {
    /// Stable id, stored in the settings.
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn roles(&self) -> &'static [SoundRole];
    fn create(&self, sample_rate: f32) -> Box<dyn Voice>;
}

struct Src<F: Fn(f32) -> Box<dyn Voice> + Sync> {
    id: &'static str,
    name: &'static str,
    roles: &'static [SoundRole],
    make: F,
}

impl<F: Fn(f32) -> Box<dyn Voice> + Sync> SoundSource for Src<F> {
    fn id(&self) -> &str {
        self.id
    }
    fn name(&self) -> &str {
        self.name
    }
    fn roles(&self) -> &'static [SoundRole] {
        self.roles
    }
    fn create(&self, sr: f32) -> Box<dyn Voice> {
        (self.make)(sr)
    }
}

static SINE_CLICK: Src<fn(f32) -> Box<dyn Voice>> =
    Src { id: "synth.click", name: "Synth click", roles: &[SoundRole::Click], make: |sr| Box::new(SineClick::new(sr)) };
static WOODBLOCK: Src<fn(f32) -> Box<dyn Voice>> =
    Src { id: "synth.woodblock", name: "Woodblock", roles: &[SoundRole::Click], make: |sr| Box::new(Woodblock::new(sr)) };
static PLUCK_BASS: Src<fn(f32) -> Box<dyn Voice>> = Src {
    id: "synth.pluck",
    name: "Synth pluck",
    roles: &[SoundRole::Click, SoundRole::Pitched],
    make: |sr| Box::new(PluckBass::new(sr)),
};
static FM_PIANO: Src<fn(f32) -> Box<dyn Voice>> =
    Src { id: "synth.epiano", name: "FM e-piano", roles: &[SoundRole::Pitched], make: |sr| Box::new(FmPiano::new(sr)) };

/// Sources added at runtime (e.g. SoundFonts); they live for the rest of the program.
static REGISTERED: RwLock<Vec<&'static dyn SoundSource>> = RwLock::new(Vec::new());

/// Adds a source, unless one with the same id exists. Returns whether it was added.
pub fn register(src: Box<dyn SoundSource>) -> bool {
    if source(src.id()).is_some() {
        return false;
    }
    let mut reg = REGISTERED.write().unwrap_or_else(|e| e.into_inner());
    reg.push(Box::leak(src));
    true
}

/// Registers every `.sf2` file in `dirs` as an instrument (`sf2:<file stem>`); earlier
/// directories win when names clash. Returns the number added.
pub fn register_soundfonts(dirs: &[PathBuf]) -> usize {
    find_soundfonts(dirs).iter().filter_map(|p| SoundFontSource::new(p)).map(|s| register(Box::new(s))).filter(|&added| added).count()
}

/// All available sound sources: the built-in ones first.
pub fn sources() -> Vec<&'static dyn SoundSource> {
    let mut all: Vec<&'static dyn SoundSource> = vec![&SINE_CLICK, &WOODBLOCK, &PLUCK_BASS, &FM_PIANO];
    all.extend(REGISTERED.read().unwrap_or_else(|e| e.into_inner()).iter().copied());
    all
}

pub fn source(id: &str) -> Option<&'static dyn SoundSource> {
    sources().into_iter().find(|s| s.id() == id)
}

/// The fallback for each role when a stored id isn't available (e.g. a removed SoundFont).
pub fn default_source(role: SoundRole) -> &'static dyn SoundSource {
    match role {
        SoundRole::Click => &SINE_CLICK,
        SoundRole::Pitched => &FM_PIANO,
    }
}

pub fn sources_for(role: SoundRole) -> impl Iterator<Item = &'static dyn SoundSource> {
    sources().into_iter().filter(move |s| s.roles().contains(&role))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_makes_sound_and_decays() {
        for src in sources() {
            let mut v = src.create(48_000.0);
            let kind = if src.roles().contains(&SoundRole::Pitched) {
                SoundKind::Note { midi: 40.0, velocity: 1.0, duration: 0.2 }
            } else {
                SoundKind::Click(ClickKind::Accent)
            };
            v.trigger(&SoundEvent { kind, gain: 1.0 });
            let mut buf = vec![0.0; 4800];
            v.render(&mut buf);
            let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.05 && peak <= 1.0, "{}: peak {peak}", src.id());
            let mut tail = vec![0.0; 48_000 * 3];
            v.render(&mut tail);
            let end = tail[tail.len() - 4800..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(end < 1e-3, "{}: tail {end}", src.id());
        }
    }

    #[test]
    fn soundfont_instrument_plays_and_releases() {
        let sr = 48_000;
        let tone: Vec<i16> = (0..sr * 4).map(|i| ((i as f32 / sr as f32 * 261.6 * std::f32::consts::TAU).sin() * 20_000.0) as i16).collect();
        let inst = sf2pack::Instrument {
            name: "Test tone".into(),
            samples: vec![sf2pack::Sample { name: "C4".into(), rate: sr as u32, root: 60, data: tone }],
            zones: vec![sf2pack::Zone { sample: 0, keys: (0, 127), vels: (0, 127) }],
            release: 0.05,
        };
        let dir = std::env::temp_dir().join(format!("bg-sound-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut file = std::fs::File::create(dir.join("test-tone.sf2")).unwrap();
        sf2pack::write(&mut file, &inst).unwrap();
        drop(file);

        assert_eq!(register_soundfonts(std::slice::from_ref(&dir)), 1);
        assert_eq!(register_soundfonts(std::slice::from_ref(&dir)), 0, "registered once");
        let src = source("sf2:test-tone").expect("registered");
        assert_eq!(src.name(), "Test tone");
        assert!(sources_for(SoundRole::Pitched).any(|s| s.id() == "sf2:test-tone"));

        let mut v = src.create(sr as f32);
        v.trigger(&SoundEvent { kind: SoundKind::Note { midi: 60.0, velocity: 1.0, duration: 0.2 }, gain: 1.0 });
        let mut buf = vec![0.0; sr / 10];
        v.render(&mut buf);
        let first = buf.iter().position(|x| x.abs() > 1e-3).unwrap();
        assert!(first < 64, "starts within ~1 ms: {first}");
        let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.1, "peak {peak}");
        // Released at 0.2 s (+ 50 ms release), long before the 4 s sample ends.
        let mut tail = vec![0.0; sr / 2];
        v.render(&mut tail);
        let end = tail[tail.len() - 4800..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(end < 1e-3, "tail {end}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The SoundFonts shipped in `assets/soundfonts`: every key of a piano sounds right
    /// away, and louder with velocity.
    #[test]
    fn bundled_soundfonts_play() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/soundfonts");
        let fonts = find_soundfonts(&[dir]);
        if fonts.is_empty() {
            eprintln!("no bundled SoundFonts; skipping");
            return;
        }
        let sr = 48_000.0;
        for path in fonts {
            let src = SoundFontSource::new(&path).unwrap();
            for key in (21..=108).step_by(5) {
                let mut peaks = Vec::new();
                for velocity in [0.2, 0.6, 1.0] {
                    let mut v = src.create(sr);
                    v.trigger(&SoundEvent { kind: SoundKind::Note { midi: key as f32, velocity, duration: 0.3 }, gain: 1.0 });
                    let mut buf = vec![0.0; 9600];
                    v.render(&mut buf);
                    let first = buf.iter().position(|x| x.abs() > 1e-3);
                    assert!(first.is_some_and(|f| f < 240), "{} key {key}: onset {first:?}", src.name());
                    peaks.push(buf.iter().fold(0.0f32, |m, x| m.max(x.abs())));
                }
                assert!(peaks.windows(2).all(|w| w[1] > w[0]) && peaks[2] <= 1.0, "{} key {key}: {peaks:?}", src.name());
            }
        }
    }
}
