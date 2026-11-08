//! Turning progression settings into concrete chords: colouring by "jazziness" and
//! seeded random progressions with common tones between neighbouring chords.

use rand::Rng;

use super::{Chord, ChordSpec, Scale};

/// Chord size suggested by `jazziness` (0 = plain triads, 1 = 13th chords), with some
/// randomness so not every chord gets the same colour.
fn jazz_size(jazziness: f32, rng: &mut impl Rng) -> u8 {
    let j = jazziness.clamp(0.0, 1.0);
    let r: f32 = rng.random();
    // Thresholds slide down as jazziness rises.
    let mut size = 3;
    if r < (j * 2.2 - 0.3).clamp(0.0, 1.0) {
        size = 4;
    }
    if r < (j * 1.8 - 0.8).clamp(0.0, 1.0) {
        size = 5;
    }
    if r < (j * 1.6 - 1.0).clamp(0.0, 1.0) {
        size = 6 + rng.random_range(0..2);
    }
    size
}

/// Adds colour to a written chord: it never gets smaller than written.
pub fn colourize(spec: ChordSpec, jazziness: f32, rng: &mut impl Rng) -> ChordSpec {
    ChordSpec {
        degree: spec.degree,
        size: spec.size.max(jazz_size(jazziness, rng)),
    }
}

/// Realizes a written progression with colour.
pub fn resolve(
    specs: &[ChordSpec],
    scale: &Scale,
    jazziness: f32,
    rng: &mut impl Rng,
) -> Vec<Chord> {
    specs
        .iter()
        .map(|&s| scale.realize(colourize(s, jazziness, rng)))
        .collect()
}

fn shares_tone(a: &Chord, b: &Chord) -> bool {
    let bt = b.tones();
    a.tones().iter().any(|t| bt.contains(t))
}

/// Weighted first chords of a random progression (degree, weight): mostly the tonic and
/// the common pre-dominants, never the diminished vii.
const STARTS: [(usize, u32); 6] = [(0, 3), (1, 2), (3, 2), (5, 2), (4, 1), (2, 1)];

/// Picks a degree from `(degree, weight)` pairs.
fn weighted(opts: &[(usize, u32)], rng: &mut impl Rng) -> usize {
    let total: u32 = opts.iter().map(|o| o.1).sum();
    let mut pick = rng.random_range(0..total.max(1));
    opts.iter()
        .find_map(|&(d, w)| {
            if pick < w {
                Some(d)
            } else {
                pick -= w;
                None
            }
        })
        .unwrap_or(0)
}

/// A seeded random in-key progression. Starts on a weighted pick (the tonic most often),
/// follows functional-harmony tendencies (more ii–V motion when jazzier), ends on the
/// first chord's dominant (a fifth above it) so it loops back, and makes each chord share
/// at least one note with the previous one: when a move would share none, the new chord
/// is first extended (a seventh or ninth usually creates the common tone), and only then
/// re-picked.
pub fn random_progression(
    scale: &Scale,
    len: usize,
    jazziness: f32,
    rng: &mut impl Rng,
) -> Vec<Chord> {
    // Weighted successors for each degree (0 = I .. 6 = VII).
    const NEXT: [&[(usize, u32)]; 7] = [
        &[(3, 3), (4, 3), (5, 3), (1, 2), (2, 1)],
        &[(4, 6), (6, 1), (3, 1)],
        &[(5, 3), (3, 2), (1, 1)],
        &[(4, 4), (0, 2), (1, 2), (6, 1)],
        &[(0, 6), (5, 2), (3, 1)],
        &[(1, 3), (3, 3), (4, 2), (2, 1)],
        &[(0, 4), (2, 1), (5, 1)],
    ];
    let len = len.max(1);
    let j = jazziness.clamp(0.0, 1.0);
    let first = weighted(&STARTS, rng);
    let home = (first + 4) % 7;
    let mut out = vec![scale.realize(colourize(
        ChordSpec {
            degree: first,
            size: 3,
        },
        j,
        rng,
    ))];
    while out.len() < len {
        let prev = out.last().unwrap().clone();
        let last = out.len() + 1 == len;
        let mut tried: Vec<usize> = Vec::new();
        let chord = loop {
            let degree = if last && len > 2 && !tried.contains(&home) && prev.degree != home {
                home
            } else {
                let opts: Vec<(usize, u32)> = NEXT[prev.degree]
                    .iter()
                    .filter(|o| !tried.contains(&o.0))
                    // Jazz: favour moves into ii and from ii into V.
                    .map(|&(d, w)| {
                        (
                            d,
                            w + if d == 1 || (prev.degree == 1 && d == 4) {
                                (j * 6.0) as u32
                            } else {
                                0
                            },
                        )
                    })
                    .collect();
                if opts.is_empty() {
                    break scale.realize(colourize(ChordSpec { degree: 4, size: 4 }, j, rng));
                }
                weighted(&opts, rng)
            };
            let mut spec = colourize(ChordSpec { degree, size: 3 }, j, rng);
            let mut chord = scale.realize(spec);
            while !shares_tone(&prev, &chord) && spec.size < 5 {
                spec.size += 1;
                chord = scale.realize(spec);
            }
            if shares_tone(&prev, &chord) {
                break chord;
            }
            tried.push(degree);
        };
        out.push(chord);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harmony::Mode;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn random_progressions_are_seeded_and_connected() {
        let mut starts = [0; 7];
        for mode in Mode::ALL {
            let scale = Scale { root: 2, mode };
            for seed in 0..40 {
                for j in [0.0, 0.5, 1.0] {
                    let a = random_progression(&scale, 8, j, &mut ChaCha8Rng::seed_from_u64(seed));
                    let b = random_progression(&scale, 8, j, &mut ChaCha8Rng::seed_from_u64(seed));
                    assert_eq!(a, b);
                    assert_eq!(a.len(), 8);
                    starts[a[0].degree] += 1;
                    // Ends on the first chord's dominant when that connects (and isn't
                    // already the chord before).
                    let home = (a[0].degree + 4) % 7;
                    let to_home = scale.realize(ChordSpec {
                        degree: home,
                        size: 5,
                    });
                    if a[6].degree != home && shares_tone(&a[6], &to_home) {
                        assert_eq!(a[7].degree, home, "{mode:?}, seed {seed}, j {j}");
                    }
                    for w in a.windows(2) {
                        assert!(
                            shares_tone(&w[0], &w[1]),
                            "{} -> {} ({mode:?}, seed {seed}, j {j})",
                            w[0].name(),
                            w[1].name()
                        );
                    }
                }
            }
        }
        // Varied starts, mostly the tonic, never vii.
        assert!(starts[0] > starts[4] && starts[6] == 0, "{starts:?}");
        assert!(starts.iter().filter(|&&n| n > 0).count() == 6, "{starts:?}");
    }

    #[test]
    fn jazziness_adds_colour() {
        let scale = Scale::default();
        let avg_size = |j: f32| {
            let mut rng = ChaCha8Rng::seed_from_u64(3);
            let chords: Vec<Chord> = (0..50)
                .flat_map(|_| random_progression(&scale, 8, j, &mut rng))
                .collect();
            chords.iter().map(|c| c.tones().len()).sum::<usize>() as f32 / chords.len() as f32
        };
        let (plain, mid, jazz) = (avg_size(0.0), avg_size(0.5), avg_size(1.0));
        assert!(plain < 3.4, "{plain}");
        assert!(plain < mid && mid < jazz, "{plain} {mid} {jazz}");
        assert!(jazz > 4.5, "{jazz}");
    }
}
