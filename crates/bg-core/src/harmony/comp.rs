//! Piano comping rhythms.

/// One piano attack, in beats from the start of the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub pos: f32,
    pub len: f32,
    pub velocity: f32,
}

/// (id, name)
pub const PATTERNS: [(&str, &str); 7] = [
    ("sustain", "Sustained"),
    ("halves", "Half notes"),
    ("stabs", "Quarter stabs"),
    ("backbeat", "2 & 4"),
    ("charleston", "Charleston"),
    ("pushes", "Off-beat pushes"),
    ("bossa", "Bossa"),
];

const fn hit(pos: f32, len: f32, velocity: f32) -> Hit {
    Hit { pos, len, velocity }
}

/// The attacks of one bar of `beats` beats.
pub fn hits(pattern: &str, beats: u32) -> Vec<Hit> {
    let b = beats.max(1) as f32;
    let mut v: Vec<Hit> = match pattern {
        "halves" => (0..beats.max(1)).step_by(2).map(|i| hit(i as f32, 2.0, if i == 0 { 0.8 } else { 0.65 })).collect(),
        "stabs" => (0..beats.max(1)).map(|i| hit(i as f32, 0.45, if i == 0 { 0.8 } else { 0.6 })).collect(),
        "backbeat" => (1..beats.max(1)).step_by(2).map(|i| hit(i as f32, 0.6, 0.7)).collect(),
        "charleston" => vec![hit(0.0, 1.0, 0.8), hit(1.5, 0.5, 0.7)],
        "pushes" => (0..beats.max(1)).map(|i| hit(i as f32 + 0.5, 0.4, 0.6)).collect(),
        "bossa" => vec![hit(0.0, 1.0, 0.75), hit(1.5, 1.0, 0.65), hit(3.0, 0.75, 0.65)],
        _ => vec![hit(0.0, b, 0.7)],
    };
    // Fit to the bar (patterns are written for 4/4).
    v.retain(|h| h.pos < b);
    for h in &mut v {
        h.len = h.len.min(b - h.pos);
    }
    if v.is_empty() {
        v.push(hit(0.0, b, 0.7));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_fit_any_meter() {
        for (id, _) in PATTERNS {
            for beats in [2, 3, 4, 5, 7] {
                let h = hits(id, beats);
                assert!(!h.is_empty(), "{id} {beats}");
                assert!(h.iter().all(|h| h.pos >= 0.0 && h.pos + h.len <= beats as f32 + 1e-6), "{id} {beats}: {h:?}");
                assert!(h.windows(2).all(|w| w[0].pos < w[1].pos));
            }
        }
    }
}
