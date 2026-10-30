//! Piano voicings with voice leading.
//!
//! The right hand plays a close (or, when jazzier, drop-2) voicing between C3 and C6,
//! choosing the inversion that moves least from the previous chord, which is what makes
//! a pianist's hand positions readable. The left hand plays the root (plain), a root +
//! seventh shell (medium), or nothing (jazzy, rootless voicings).

use rand::Rng;

use super::Chord;

pub const RH_LOW: u8 = 48;
pub const RH_HIGH: u8 = 84;
const LH_LOW: u8 = 36;

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
    let near = |n: u8, set: &[u8]| set.iter().map(|&m| (n as i32 - m as i32).unsigned_abs()).min().unwrap_or(0) as f32;
    a.iter().map(|&n| near(n, b)).sum::<f32>() + b.iter().map(|&n| near(n, a)).sum::<f32>()
}

fn mean(v: &[u8]) -> f32 {
    v.iter().map(|&n| n as f32).sum::<f32>() / v.len().max(1) as f32
}

/// Voices `chord` for piano. `prev` is the previous voicing (for voice leading).
/// Returns ascending MIDI notes.
pub fn voice(chord: &Chord, prev: Option<&[u8]>, jazziness: f32, rng: &mut impl Rng) -> Vec<u8> {
    let j = jazziness.clamp(0.0, 1.0);
    let pcs = rh_pitch_classes(chord, j);
    let prev_rh: Vec<u8> = prev.map(|p| p.iter().copied().filter(|&n| n >= RH_LOW).collect()).unwrap_or_default();

    let mut best: Option<(f32, Vec<u8>)> = None;
    for r in 0..pcs.len() {
        let mut order = pcs.clone();
        order.rotate_left(r);
        for bottom in RH_LOW..RH_LOW + 20 {
            if bottom % 12 != order[0] {
                continue;
            }
            let close = close_voicing(&order, bottom);
            let mut candidates = vec![close.clone()];
            if j >= 0.5 && close.len() >= 4 {
                let mut drop2 = close.clone();
                let i = drop2.len() - 2;
                drop2[i] -= 12;
                drop2.sort_unstable();
                candidates.push(drop2);
            }
            for c in candidates {
                if c[0] < RH_LOW || *c.last().unwrap() > RH_HIGH {
                    continue;
                }
                let centre = (mean(&c) - 64.0).abs();
                let mut cost = if prev_rh.is_empty() {
                    centre + if r != 0 && j < 0.25 { 3.0 } else { 0.0 }
                } else {
                    movement(&c, &prev_rh) + 0.3 * centre
                };
                cost += rng.random::<f32>() * j * 2.0;
                if best.as_ref().is_none_or(|b| cost < b.0) {
                    best = Some((cost, c));
                }
            }
        }
    }
    let mut notes = best.map(|b| b.1).unwrap_or_else(|| close_voicing(&pcs, RH_LOW + pcs[0] % 12));

    // Left hand.
    let rh_low = notes[0];
    let root = LH_LOW + (chord.root() + 12 - LH_LOW % 12) % 12;
    if j < 0.75 && root < rh_low {
        let mut lh = vec![root];
        if j >= 0.5 {
            let shell = chord.seventh.unwrap_or(chord.third());
            let n = root + (shell + 12 - chord.root()) % 12;
            if n < rh_low {
                lh.push(n);
            }
        }
        notes.splice(0..0, lh);
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harmony::{ChordSpec, Mode, Scale};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    const C: Scale = Scale { root: 0, mode: Mode::Ionian };

    fn chain(chords: &[Chord], j: f32) -> Vec<Vec<u8>> {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut out: Vec<Vec<u8>> = Vec::new();
        for c in chords {
            let v = voice(c, out.last().map(|v| v.as_slice()), j, &mut rng);
            out.push(v);
        }
        out
    }

    #[test]
    fn voicings_stay_in_range_and_keep_guide_tones() {
        for mode in Mode::ALL {
            let scale = Scale { root: 5, mode };
            for size in 3..=7 {
                for j in [0.0, 0.3, 0.6, 0.9] {
                    let chords: Vec<Chord> = (0..7).map(|d| scale.realize(ChordSpec { degree: d, size })).collect();
                    for (c, v) in chords.iter().zip(chain(&chords, j)) {
                        assert!(v.iter().all(|&n| (LH_LOW..=RH_HIGH).contains(&n)), "{} {v:?}", c.name());
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
        let chords: Vec<Chord> = [1, 4, 0, 0].iter().map(|&d| C.realize(ChordSpec { degree: d, size: 4 })).collect();
        let led = chain(&chords, 0.3);
        let rh = |v: &Vec<u8>| v.iter().copied().filter(|&n| n >= RH_LOW).collect::<Vec<u8>>();
        let led_moves: f32 = led.windows(2).map(|w| movement(&rh(&w[0]), &rh(&w[1]))).sum();
        let root_pos: Vec<Vec<u8>> = chords.iter().map(|c| close_voicing(&c.tones(), 48 + c.root())).collect();
        let root_moves: f32 = root_pos.windows(2).map(|w| movement(&w[0], &w[1])).sum();
        assert!(led_moves < root_moves, "led {led_moves} vs root position {root_moves}: {led:?}");
        // ii7 -> V7 with good voice leading moves only a couple of semitones in total per voice.
        assert!(movement(&rh(&led[0]), &rh(&led[1])) <= 8.0, "{led:?}");
    }
}
