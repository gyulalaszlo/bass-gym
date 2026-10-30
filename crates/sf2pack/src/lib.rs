//! A minimal SoundFont 2.01 writer for one-shot, multi-sampled instruments: one preset
//! with one instrument, mono 16-bit samples, key × velocity zones.

use std::io::{self, Write};

/// One recorded sample (mono).
pub struct Sample {
    pub name: String,
    pub rate: u32,
    /// MIDI note the sample was recorded at.
    pub root: u8,
    pub data: Vec<i16>,
}

/// Plays `sample` for keys `keys.0..=keys.1` and velocities `vels.0..=vels.1`.
pub struct Zone {
    pub sample: usize,
    pub keys: (u8, u8),
    pub vels: (u8, u8),
}

pub struct Instrument {
    pub name: String,
    pub samples: Vec<Sample>,
    pub zones: Vec<Zone>,
    /// Release time after note-off, in seconds.
    pub release: f32,
}

// Generator operators (SoundFont 2.04, section 8.1.2).
const GEN_RELEASE_VOL_ENV: u16 = 38;
const GEN_INSTRUMENT: u16 = 41;
const GEN_KEY_RANGE: u16 = 43;
const GEN_VEL_RANGE: u16 = 44;
const GEN_SAMPLE_ID: u16 = 53;
const GEN_SAMPLE_MODES: u16 = 54;
const GEN_ROOT_KEY: u16 = 58;
/// The default "velocity → initial attenuation" modulator (negative concave velocity).
const MOD_VELOCITY: u16 = 0x0502;
const GEN_INITIAL_ATTENUATION: u16 = 48;
/// Zero padding required after every sample.
const SAMPLE_PAD: usize = 46;

fn name20(s: &str) -> [u8; 20] {
    let mut out = [0u8; 20];
    for (o, b) in out.iter_mut().zip(s.bytes().filter(u8::is_ascii).take(19)) {
        *o = b;
    }
    out
}

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 9);
    v.extend_from_slice(id);
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(body);
    if body.len() % 2 == 1 {
        v.push(0);
    }
    v
}

fn list(kind: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut body = kind.to_vec();
    chunks.iter().for_each(|c| body.extend_from_slice(c));
    chunk(b"LIST", &body)
}

fn zstr(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.bytes().filter(u8::is_ascii).collect();
    v.push(0);
    if v.len() % 2 == 1 {
        v.push(0);
    }
    v
}

struct Bytes(Vec<u8>);

impl Bytes {
    fn u8(&mut self, x: u8) -> &mut Self {
        self.0.push(x);
        self
    }
    fn u16(&mut self, x: u16) -> &mut Self {
        self.0.extend_from_slice(&x.to_le_bytes());
        self
    }
    fn u32(&mut self, x: u32) -> &mut Self {
        self.0.extend_from_slice(&x.to_le_bytes());
        self
    }
    fn name(&mut self, s: &str) -> &mut Self {
        self.0.extend_from_slice(&name20(s));
        self
    }
    fn gen_range(&mut self, op: u16, (lo, hi): (u8, u8)) -> &mut Self {
        self.u16(op).u8(lo).u8(hi)
    }
}

/// Seconds → SoundFont timecents.
fn timecents(sec: f32) -> i16 {
    (1200.0 * sec.max(0.001).log2()).round() as i16
}

/// Writes `inst` as a SoundFont with a single preset (bank 0, program 0).
pub fn write(out: &mut impl Write, inst: &Instrument) -> io::Result<()> {
    // Sample data and headers.
    let mut smpl = Vec::new();
    let mut shdr = Bytes(Vec::new());
    for s in &inst.samples {
        let start = (smpl.len() / 2) as u32;
        s.data.iter().for_each(|x| smpl.extend_from_slice(&x.to_le_bytes()));
        smpl.resize(smpl.len() + SAMPLE_PAD * 2, 0);
        let end = start + s.data.len() as u32;
        shdr.name(&s.name).u32(start).u32(end).u32(start).u32(end).u32(s.rate).u8(s.root).u8(0).u16(0).u16(1);
    }
    shdr.name("EOS").u32(0).u32(0).u32(0).u32(0).u32(0).u8(0).u8(0).u16(0).u16(0);

    // Instrument zones. Each zone switches off the default velocity → attenuation
    // modulator: the recorded layers already carry the dynamics.
    let (mut ibag, mut imod, mut igen) = (Bytes(Vec::new()), Bytes(Vec::new()), Bytes(Vec::new()));
    let (mut ngen, mut nmod) = (0u16, 0u16);
    for z in &inst.zones {
        ibag.u16(ngen).u16(nmod);
        imod.u16(MOD_VELOCITY).u16(GEN_INITIAL_ATTENUATION).u16(0).u16(0).u16(0);
        nmod += 1;
        igen.gen_range(GEN_KEY_RANGE, z.keys).gen_range(GEN_VEL_RANGE, z.vels);
        igen.u16(GEN_ROOT_KEY).u16(inst.samples[z.sample].root as u16);
        igen.u16(GEN_RELEASE_VOL_ENV).u16(timecents(inst.release) as u16);
        igen.u16(GEN_SAMPLE_MODES).u16(0);
        igen.u16(GEN_SAMPLE_ID).u16(z.sample as u16);
        ngen += 6;
    }
    ibag.u16(ngen).u16(nmod);
    imod.u16(0).u16(0).u16(0).u16(0).u16(0);
    igen.u32(0);
    let mut instr = Bytes(Vec::new());
    instr.name(&inst.name).u16(0).name("EOI").u16(inst.zones.len() as u16);

    // One preset with one zone pointing at the instrument.
    let mut phdr = Bytes(Vec::new());
    phdr.name(&inst.name).u16(0).u16(0).u16(0).u32(0).u32(0).u32(0);
    phdr.name("EOP").u16(0).u16(0).u16(1).u32(0).u32(0).u32(0);
    let mut pbag = Bytes(Vec::new());
    pbag.u16(0).u16(0).u16(1).u16(0);
    let mut pmod = Bytes(Vec::new());
    pmod.u16(0).u16(0).u16(0).u16(0).u16(0);
    let mut pgen = Bytes(Vec::new());
    pgen.u16(GEN_INSTRUMENT).u16(0).u32(0);

    let mut ifil = Bytes(Vec::new());
    ifil.u16(2).u16(1);
    let info = list(
        b"INFO",
        &[chunk(b"ifil", &ifil.0), chunk(b"isng", &zstr("EMU8000")), chunk(b"INAM", &zstr(&inst.name)), chunk(b"ISFT", &zstr("bass-gym sf2pack"))],
    );
    let sdta = list(b"sdta", &[chunk(b"smpl", &smpl)]);
    let pdta = list(
        b"pdta",
        &[
            chunk(b"phdr", &phdr.0),
            chunk(b"pbag", &pbag.0),
            chunk(b"pmod", &pmod.0),
            chunk(b"pgen", &pgen.0),
            chunk(b"inst", &instr.0),
            chunk(b"ibag", &ibag.0),
            chunk(b"imod", &imod.0),
            chunk(b"igen", &igen.0),
            chunk(b"shdr", &shdr.0),
        ],
    );
    let mut body = b"sfbk".to_vec();
    for part in [info, sdta, pdta] {
        body.extend_from_slice(&part);
    }
    out.write_all(&chunk(b"RIFF", &body))
}

/// Splits the keyboard between sample roots: each root covers the keys closest to it,
/// and the outermost roots extend to the ends of the MIDI range.
pub fn key_ranges(roots: &[u8]) -> Vec<(u8, u8)> {
    let mut sorted = roots.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let n = sorted.len();
    (0..n)
        .map(|i| {
            let lo = if i == 0 { 0 } else { ((sorted[i - 1] as u16 + sorted[i] as u16) / 2) as u8 + 1 };
            let hi = if i + 1 == n { 127 } else { ((sorted[i] as u16 + sorted[i + 1] as u16) / 2) as u8 };
            (lo, hi)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_keys_between_roots() {
        assert_eq!(key_ranges(&[48, 41, 55]), vec![(0, 44), (45, 51), (52, 127)]);
    }

    /// The written file loads in a SoundFont player and picks zones by key and velocity.
    #[test]
    fn written_soundfont_plays() {
        let sr = 48_000;
        let tone = |freq: f32, amp: f32| -> Vec<i16> {
            (0..sr).map(|i| ((i as f32 / sr as f32 * freq * std::f32::consts::TAU).sin() * amp * 32_000.0) as i16).collect()
        };
        let inst = Instrument {
            name: "Test piano".into(),
            samples: vec![
                Sample { name: "C4 soft".into(), rate: sr, root: 60, data: tone(261.6, 0.1) },
                Sample { name: "C4 loud".into(), rate: sr, root: 60, data: tone(261.6, 0.9) },
            ],
            zones: vec![Zone { sample: 0, keys: (0, 127), vels: (0, 63) }, Zone { sample: 1, keys: (0, 127), vels: (64, 127) }],
            release: 0.2,
        };
        let mut bytes = Vec::new();
        write(&mut bytes, &inst).unwrap();
        let sf = std::sync::Arc::new(rustysynth::SoundFont::new(&mut bytes.as_slice()).unwrap());
        assert_eq!(sf.get_presets()[0].get_name(), "Test piano");
        assert_eq!(sf.get_instruments()[0].get_regions().len(), 2);

        let peak = |vel: i32| {
            let mut settings = rustysynth::SynthesizerSettings::new(sr as i32);
            settings.enable_reverb_and_chorus = false;
            let mut synth = rustysynth::Synthesizer::new(&sf, &settings).unwrap();
            synth.note_on(0, 60, vel);
            let (mut l, mut r) = (vec![0.0; 4800], vec![0.0; 4800]);
            synth.render(&mut l, &mut r);
            l.iter().fold(0.0f32, |m, x| m.max(x.abs()))
        };
        let (soft, loud) = (peak(127 / 4), peak(127));
        assert!(soft > 0.0 && loud > soft * 3.0, "soft {soft}, loud {loud}");
    }
}
