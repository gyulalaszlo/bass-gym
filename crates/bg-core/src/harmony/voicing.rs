//! Piano voicings with voice leading.
//!
//! One hand (the narrow default): the right hand plays a close (or, when jazzier, drop-2)
//! voicing around the centre octave, choosing the inversion that moves least from the
//! previous chord, which is what makes a pianist's hand positions readable. The left
//! hand plays the root (plain), a root + seventh shell (medium), or nothing (jazzy,
//! rootless voicings).
//!
//! Wider voicings ([`VoicingStyle::width`]) use two hands: the left hand plays a root +
//! fifth (or a shell / tenth when jazzier) an octave or so below, the right hand moves
//! up; "stacked" voicings also double the bass and the top voice in octaves.

use rand::Rng;

use super::Chord;

pub const RH_LOW: u8 = 48;
pub const RH_HIGH: u8 = 84;
const LH_LOW: u8 = 36;
/// Centre octave of the default voicings (around E4).
pub const CENTER_OCTAVE: i8 = 4;
/// How far up the right hand moves when both hands play.
const TWO_HAND_RH_SHIFT: i32 = 7;

/// How the keyboard player voices chords.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoicingStyle {
    /// 0 = plain triads … 1 = rootless, coloured voicings.
    pub jazziness: f32,
    /// 0 = one hand, 1 = two hands, 2 = two hands with octave doublings. Fractions mix
    /// the neighbouring widths chord by chord.
    pub width: f32,
    /// Moves everything by octaves ([`CENTER_OCTAVE`] = the default register).
    pub center_octave: i8,
}

impl Default for VoicingStyle {
    fn default() -> Self {
        Self {
            jazziness: 0.0,
            width: 0.0,
            center_octave: CENTER_OCTAVE,
        }
    }
}

/// A chord as played by both hands (each ascending).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Voicing {
    pub lh: Vec<u8>,
    pub rh: Vec<u8>,
}

impl Voicing {
    /// All notes, ascending, without duplicates.
    pub fn notes(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self.lh.iter().chain(&self.rh).copied().collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

fn rh_pitch_classes(chord: &Chord, j: f32) -> Vec<u8> {
    let third = chord.third();
    let fifth = chord.triad[2];
    let root = chord.root();
    let colours: Vec<u8> = chord.colours.iter().map(|c| c.1).collect();
    // Priority: guide tones, colours, fifth, root.
    let mut wanted: Vec<u8> = Vec::new();
    if j < 0.5 {
        wanted.extend([root, third, fifth]);
        wanted.extend(chord.seventh);
        wanted.extend(&colours);
    } else {
        wanted.push(third);
        wanted.extend(chord.seventh);
        wanted.extend(&colours);
        wanted.push(fifth);
        if j < 0.75 {
            wanted.push(root);
        }
    }
    let mut pcs: Vec<u8> = Vec::new();
    for pc in wanted {
        if !pcs.contains(&pc) && pcs.len() < if j < 0.5 { 4 } else { 5 } {
            pcs.push(pc);
        }
    }
    // At least three notes (e.g. a rootless triad gets its fifth and root back).
    for p in [fifth, root] {
        if pcs.len() < 3 && !pcs.contains(&p) {
            pcs.push(p);
        }
    }
    // Stacking order (root, 3, 5, 7, 9, ...) for building close voicings.
    let order = chord.tones();
    pcs.sort_by_key(|p| order.iter().position(|o| o == p).unwrap_or(99));
    pcs
}

fn close_voicing(order: &[u8], bottom: u8) -> Vec<u8> {
    let mut notes = vec![bottom];
    for &pc in &order[1..] {
        let prev = *notes.last().unwrap();
        let mut n = prev + 1;
        while n % 12 != pc {
            n += 1;
        }
        notes.push(n);
    }
    notes
}

/// Sum over notes of the distance to the nearest note of the other chord, both ways.
fn movement(a: &[u8], b: &[u8]) -> f32 {
    let near = |n: u8, set: &[u8]| {
        set.iter()
            .map(|&m| (n as i32 - m as i32).unsigned_abs())
            .min()
            .unwrap_or(0) as f32
    };
    a.iter().map(|&n| near(n, b)).sum::<f32>() + b.iter().map(|&n| near(n, a)).sum::<f32>()
}

fn mean(v: &[u8]) -> f32 {
    v.iter().map(|&n| n as f32).sum::<f32>() / v.len().max(1) as f32
}

fn midi(n: i32) -> u8 {
    n.clamp(0, 127) as u8
}

/// The lowest note of pitch class `pc` at or above `low`.
fn at_or_above(pc: u8, low: u8) -> u8 {
    low + (pc + 12 - low % 12) % 12
}

/// The right-hand voicing of `pcs` within `low..=high` (bottom note below `low + 20`)
/// that moves least from `prev_rh` (or sits closest to `centre` when there is none).
#[allow(clippy::too_many_arguments)]
fn right_hand(
    pcs: &[u8],
    low: u8,
    high: u8,
    centre: f32,
    prev_rh: &[u8],
    j: f32,
    drop2: bool,
    rng: &mut impl Rng,
) -> Vec<u8> {
    let mut best: Option<(f32, Vec<u8>)> = None;
    for r in 0..pcs.len() {
        let mut order = pcs.to_vec();
        order.rotate_left(r);
        for bottom in low..low.saturating_add(20) {
            if bottom % 12 != order[0] {
                continue;
            }
            let close = close_voicing(&order, bottom);
            let mut candidates = vec![close.clone()];
            if drop2 && close.len() >= 4 {
                let mut d = close.clone();
                let i = d.len() - 2;
                d[i] -= 12;
                d.sort_unstable();
                candidates.push(d);
            }
            for c in candidates {
                if c[0] < low || *c.last().unwrap() > high {
                    continue;
                }
                let off_centre = (mean(&c) - centre).abs();
                let mut cost = if prev_rh.is_empty() {
                    off_centre + if r != 0 && j < 0.25 { 3.0 } else { 0.0 }
                } else {
                    movement(&c, prev_rh) + 0.3 * off_centre
                };
                cost += rng.random::<f32>() * j * 2.0;
                if best.as_ref().is_none_or(|b| cost < b.0) {
                    best = Some((cost, c));
                }
            }
        }
    }
    best.map(|b| b.1)
        .unwrap_or_else(|| close_voicing(pcs, at_or_above(pcs[0], low)))
}

/// Voices `chord` for piano. `prev` is the previous voicing (for voice leading).
pub fn voice(
    chord: &Chord,
    prev: Option<&Voicing>,
    style: &VoicingStyle,
    rng: &mut impl Rng,
) -> Voicing {
    let j = style.jazziness.clamp(0.0, 1.0);
    let shift = 12 * (style.center_octave as i32 - CENTER_OCTAVE as i32);
    // Width level of this chord: fractions pick the wider level with that probability.
    let w = style.width.clamp(0.0, 2.0);
    let mut level = w.floor() as u8;
    if w.fract() > 0.0 && rng.random::<f32>() < w.fract() {
        level += 1;
    }
    let level = level.min(2);
    let prev_rh: &[u8] = prev.map_or(&[], |p| &p.rh);
    let root_pc = chord.root();

    if level == 0 {
        let pcs = rh_pitch_classes(chord, j);
        let (low, high) = (midi(RH_LOW as i32 + shift), midi(RH_HIGH as i32 + shift));
        let rh = right_hand(
            &pcs,
            low,
            high,
            (64 + shift) as f32,
            prev_rh,
            j,
            j >= 0.5,
            rng,
        );
        let root = at_or_above(root_pc, midi(LH_LOW as i32 + shift));
        let mut lh = Vec::new();
        if j < 0.75 && root < rh[0] {
            lh.push(root);
            if j >= 0.5 {
                let shell = chord.seventh.unwrap_or(chord.third());
                let n = root + (shell + 12 - root_pc) % 12;
                if n < rh[0] {
                    lh.push(n);
                }
            }
        }
        return Voicing { lh, rh };
    }

    // Two hands: the left hand covers the bass, so the right hand can leave out the root.
    let mut pcs = rh_pitch_classes(chord, j);
    if pcs.len() >= 4 && pcs[0] == root_pc {
        pcs.remove(0);
    }
    let rh_shift = shift + TWO_HAND_RH_SHIFT;
    let (low, high) = (
        midi(RH_LOW as i32 + rh_shift),
        midi(RH_HIGH as i32 + rh_shift),
    );
    let mut rh = right_hand(
        &pcs,
        low,
        high,
        (64 + rh_shift) as f32,
        prev_rh,
        j,
        true,
        rng,
    );
    let interval = |pc: u8| (pc + 12 - root_pc) % 12;
    let root = at_or_above(root_pc, midi(LH_LOW as i32 + shift));
    let mut lh: Vec<u8> = if j < 0.75 {
        let upper = if j < 0.5 {
            interval(chord.triad[2])
        } else {
            interval(chord.seventh.unwrap_or(chord.triad[2]))
        };
        let mut v = vec![root, root + upper];
        if j >= 0.5 {
            v.push(root + 12 + interval(chord.third())); // the tenth
        }
        v
    } else {
        // Rootless: a third / seventh shell.
        let third = root + interval(chord.third());
        let mut v = vec![third];
        if let Some(s) = chord.seventh {
            v.push(third + (s + 12 - chord.third()) % 12);
        }
        v
    };
    lh.retain(|&n| n < rh[0]);

    if level == 2 {
        if j < 0.75 && root >= 33 {
            lh.insert(0, root - 12);
        }
        let top = *rh.last().unwrap();
        if top <= 96 {
            rh.push(top + 12);
        }
    }
    Voicing { lh, rh }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harmony::{ChordSpec, Mode, Scale};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    const C: Scale = Scale {
        root: 0,
        mode: Mode::Ionian,
    };

    fn chain_styled(chords: &[Chord], style: VoicingStyle) -> Vec<Voicing> {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut out: Vec<Voicing> = Vec::new();
        for c in chords {
            let v = voice(c, out.last(), &style, &mut rng);
            out.push(v);
        }
        out
    }

    fn chain(chords: &[Chord], j: f32) -> Vec<Vec<u8>> {
        let style = VoicingStyle {
            jazziness: j,
            ..Default::default()
        };
        chain_styled(chords, style)
            .into_iter()
            .map(|v| v.notes())
            .collect()
    }

    fn diatonic(mode: Mode, size: u8) -> Vec<Chord> {
        let scale = Scale { root: 5, mode };
        (0..7)
            .map(|d| scale.realize(ChordSpec { degree: d, size }))
            .collect()
    }

    #[test]
    fn wide_voicings_use_two_hands_and_double() {
        for mode in Mode::ALL {
            for size in 3..=5 {
                for j in [0.0, 0.6, 0.9] {
                    let chords = diatonic(mode, size);
                    for width in [1.0, 2.0] {
                        let style = VoicingStyle {
                            jazziness: j,
                            width,
                            ..Default::default()
                        };
                        for (c, v) in chords.iter().zip(chain_styled(&chords, style)) {
                            let n = v.notes();
                            assert!(!v.lh.is_empty(), "{} {v:?}", c.name());
                            assert!(v.lh.iter().all(|&l| l < v.rh[0]), "{v:?}");
                            assert!(n.iter().all(|&m| (21..=108).contains(&m)), "{v:?}");
                            let pcs: Vec<u8> = n.iter().map(|m| m % 12).collect();
                            assert!(pcs.contains(&c.third()), "{} {v:?}", c.name());
                            let span = n.last().unwrap() - n[0];
                            assert!(span >= 18, "two hands span 1.5 octaves: {v:?}");
                            if width == 2.0 {
                                let top = *v.rh.last().unwrap();
                                assert!(v.rh.contains(&(top - 12)), "top doubled: {v:?}");
                                assert!(span >= 24, "{v:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn centre_octave_moves_the_voicings() {
        let chords = diatonic(Mode::Ionian, 4);
        let avg = |octave: i8| {
            let style = VoicingStyle {
                jazziness: 0.3,
                center_octave: octave,
                ..Default::default()
            };
            let v: Vec<u8> = chain_styled(&chords, style)
                .iter()
                .flat_map(|v| v.notes())
                .collect();
            mean(&v)
        };
        for (o, d) in [(3, -12.0), (5, 12.0)] {
            let shift = avg(o) - avg(CENTER_OCTAVE);
            assert!((shift - d).abs() < 3.0, "octave {o}: shift {shift}");
        }
    }

    #[test]
    fn voicings_stay_in_range_and_keep_guide_tones() {
        for mode in Mode::ALL {
            let scale = Scale { root: 5, mode };
            for size in 3..=7 {
                for j in [0.0, 0.3, 0.6, 0.9] {
                    let chords: Vec<Chord> = (0..7)
                        .map(|d| scale.realize(ChordSpec { degree: d, size }))
                        .collect();
                    for (c, v) in chords.iter().zip(chain(&chords, j)) {
                        assert!(
                            v.iter().all(|&n| (LH_LOW..=RH_HIGH).contains(&n)),
                            "{} {v:?}",
                            c.name()
                        );
                        assert!(v.windows(2).all(|w| w[0] < w[1]), "{v:?}");
                        let pcs: Vec<u8> = v.iter().map(|n| n % 12).collect();
                        assert!(pcs.contains(&c.third()), "{} {v:?}", c.name());
                        if let Some(s) = c.seventh {
                            assert!(pcs.contains(&s), "{} {v:?}", c.name());
                        }
                        if j >= 0.75 {
                            assert!(v[0] >= RH_LOW, "rootless at high jazziness: {v:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn voice_leading_moves_less_than_root_position() {
        let chords: Vec<Chord> = [1, 4, 0, 0]
            .iter()
            .map(|&d| C.realize(ChordSpec { degree: d, size: 4 }))
            .collect();
        let led = chain(&chords, 0.3);
        let rh = |v: &Vec<u8>| {
            v.iter()
                .copied()
                .filter(|&n| n >= RH_LOW)
                .collect::<Vec<u8>>()
        };
        let led_moves: f32 = led
            .windows(2)
            .map(|w| movement(&rh(&w[0]), &rh(&w[1])))
            .sum();
        let root_pos: Vec<Vec<u8>> = chords
            .iter()
            .map(|c| close_voicing(&c.tones(), 48 + c.root()))
            .collect();
        let root_moves: f32 = root_pos.windows(2).map(|w| movement(&w[0], &w[1])).sum();
        assert!(
            led_moves < root_moves,
            "led {led_moves} vs root position {root_moves}: {led:?}"
        );
        // ii7 -> V7 with good voice leading moves only a couple of semitones in total per voice.
        assert!(movement(&rh(&led[0]), &rh(&led[1])) <= 8.0, "{led:?}");
    }
}
