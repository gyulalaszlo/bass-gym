//! Grooves: step-based rhythmic patterns that repeat every `bars` bars.

use serde::{Deserialize, Serialize};

use crate::music::TimeSignature;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GrooveNote {
    pub step: u32,
    pub len_steps: u32,
    pub accent: bool,
}

impl Default for GrooveNote {
    fn default() -> Self {
        Self {
            step: 0,
            len_steps: 1,
            accent: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Groove {
    pub name: String,
    /// Grid resolution: steps per beat (beat = time signature denominator).
    pub steps_per_beat: u32,
    pub bars: u32,
    pub notes: Vec<GrooveNote>,
}

impl Default for Groove {
    fn default() -> Self {
        builtin_grooves(TimeSignature::default()).remove(0)
    }
}

/// A resolved note of one bar, in ticks relative to the bar start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarNote {
    pub pos: u32,
    pub len: u32,
    pub accent: bool,
}

impl Groove {
    pub fn steps_per_bar(&self, sig: TimeSignature) -> u32 {
        self.steps_per_beat.max(1) * sig.num.max(1)
    }

    pub fn total_steps(&self, sig: TimeSignature) -> u32 {
        self.steps_per_bar(sig) * self.bars.max(1)
    }

    pub fn ticks_per_step(&self, sig: TimeSignature) -> u32 {
        sig.ticks_per_beat() / self.steps_per_beat.max(1)
    }

    /// Notes of bar `bar_index` (wrapping around the groove length), sorted by position.
    pub fn bar_notes(&self, sig: TimeSignature, bar_index: u32) -> Vec<BarNote> {
        let spb = self.steps_per_bar(sig);
        let total = self.total_steps(sig);
        let groove_bar = bar_index % self.bars.max(1);
        let tps = self.ticks_per_step(sig);
        let mut out: Vec<BarNote> = self
            .notes
            .iter()
            .filter(|n| n.step < total && n.step / spb == groove_bar)
            .map(|n| BarNote {
                pos: (n.step % spb) * tps,
                len: n.len_steps.clamp(1, total - n.step) * tps,
                accent: n.accent,
            })
            .collect();
        out.sort_by_key(|n| n.pos);
        out
    }

    /// Index of the note covering `step` (start or sustain), if any.
    pub fn note_at(&self, step: u32) -> Option<usize> {
        self.notes
            .iter()
            .position(|n| step >= n.step && step < n.step + n.len_steps.max(1))
    }

    /// Toggle a note start at `step`: removes a note starting there, otherwise inserts a
    /// one-step note (shortening any note that was sustaining over the step).
    pub fn toggle_step(&mut self, step: u32) {
        if let Some(i) = self.notes.iter().position(|n| n.step == step) {
            self.notes.remove(i);
            return;
        }
        if let Some(i) = self.note_at(step) {
            self.notes[i].len_steps = step - self.notes[i].step;
        }
        self.notes.push(GrooveNote {
            step,
            len_steps: 1,
            accent: false,
        });
        self.notes.sort_by_key(|n| n.step);
    }

    /// Extend the closest preceding note so that it sustains through `step`.
    pub fn extend_to(&mut self, step: u32) {
        if let Some(n) = self
            .notes
            .iter_mut()
            .filter(|n| n.step <= step)
            .max_by_key(|n| n.step)
        {
            n.len_steps = step - n.step + 1;
        }
    }

    pub fn toggle_accent(&mut self, step: u32) {
        if let Some(i) = self.note_at(step) {
            self.notes[i].accent = !self.notes[i].accent;
        }
    }

    /// Change the grid resolution, keeping notes at musically equivalent positions where possible.
    pub fn set_steps_per_beat(&mut self, new_spb: u32) {
        let old = self.steps_per_beat.max(1);
        let new_spb = new_spb.max(1);
        let mut notes: Vec<GrooveNote> = self
            .notes
            .iter()
            .filter(|n| (n.step * new_spb).is_multiple_of(old))
            .map(|n| GrooveNote {
                step: n.step * new_spb / old,
                len_steps: (n.len_steps * new_spb / old).max(1),
                accent: n.accent,
            })
            .collect();
        notes.dedup_by_key(|n| n.step);
        self.notes = notes;
        self.steps_per_beat = new_spb;
    }
}

fn pattern(name: &str, sig: TimeSignature, spb: u32, per_beat: &[(u32, u32, bool)]) -> Groove {
    let mut notes = Vec::new();
    for beat in 0..sig.num.max(1) {
        for &(step, len, accent) in per_beat {
            notes.push(GrooveNote {
                step: beat * spb + step,
                len_steps: len,
                accent: accent && beat == 0,
            });
        }
    }
    Groove {
        name: name.to_owned(),
        steps_per_beat: spb,
        bars: 1,
        notes,
    }
}

/// Built-in groove library, generated for the given time signature.
pub fn builtin_grooves(sig: TimeSignature) -> Vec<Groove> {
    let mut v = vec![
        pattern("Quarter notes", sig, 1, &[(0, 1, true)]),
        pattern("Eighth notes", sig, 2, &[(0, 1, true), (1, 1, false)]),
        pattern("Offbeat eighths", sig, 2, &[(1, 1, false)]),
        pattern("Staccato eighths", sig, 4, &[(0, 1, true), (2, 1, false)]),
        pattern("Shuffle", sig, 3, &[(0, 2, true), (2, 1, false)]),
        pattern(
            "Triplets",
            sig,
            3,
            &[(0, 1, true), (1, 1, false), (2, 1, false)],
        ),
        pattern(
            "Sixteenth notes",
            sig,
            4,
            &[(0, 1, true), (1, 1, false), (2, 1, false), (3, 1, false)],
        ),
        pattern(
            "Gallop (8th + 2×16th)",
            sig,
            4,
            &[(0, 2, true), (2, 1, false), (3, 1, false)],
        ),
    ];
    // Two-bar syncopated funk figure on a 16th grid.
    let spb = 4;
    let steps_per_bar = spb * sig.num.max(1);
    let figure = [
        (0u32, 3u32, true),
        (3, 1, false),
        (6, 2, false),
        (10, 1, false),
        (11, 1, false),
        (14, 2, false),
    ];
    let mut notes = Vec::new();
    for bar in 0..2 {
        for &(s, l, a) in &figure {
            let s = s % steps_per_bar;
            notes.push(GrooveNote {
                step: bar * steps_per_bar + s,
                len_steps: l,
                accent: a,
            });
        }
    }
    notes.dedup_by_key(|n| n.step);
    v.push(Groove {
        name: "Syncopated funk".into(),
        steps_per_beat: spb,
        bars: 2,
        notes,
    });
    v.push(Groove {
        name: "Dotted quarter + eighth".into(),
        steps_per_beat: 2,
        bars: 1,
        notes: (0..sig.num.max(1) * 2)
            .step_by(4)
            .flat_map(|s| {
                [
                    GrooveNote {
                        step: s,
                        len_steps: 3,
                        accent: s == 0,
                    },
                    GrooveNote {
                        step: s + 3,
                        len_steps: 1,
                        accent: false,
                    },
                ]
            })
            .filter(|n| n.step < sig.num.max(1) * 2)
            .collect(),
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::PPQ;

    #[test]
    fn eighths_expand() {
        let sig = TimeSignature::default();
        let g = &builtin_grooves(sig)[1];
        let notes = g.bar_notes(sig, 0);
        assert_eq!(notes.len(), 8);
        assert_eq!(notes[1].pos, PPQ / 2);
        assert_eq!(notes[1].len, PPQ / 2);
        assert!(notes[0].accent && !notes[2].accent);
    }

    #[test]
    fn two_bar_groove_wraps() {
        let sig = TimeSignature::default();
        let g = builtin_grooves(sig)
            .into_iter()
            .find(|g| g.bars == 2)
            .unwrap();
        assert_eq!(g.bar_notes(sig, 0), g.bar_notes(sig, 2));
        assert!(!g.bar_notes(sig, 1).is_empty());
    }

    #[test]
    fn editing() {
        let sig = TimeSignature::default();
        let mut g = Groove {
            name: "x".into(),
            steps_per_beat: 4,
            bars: 1,
            notes: vec![],
        };
        g.toggle_step(0);
        g.extend_to(3);
        assert_eq!(g.notes[0].len_steps, 4);
        g.toggle_step(2); // splits the sustained note
        assert_eq!(g.notes[0].len_steps, 2);
        assert_eq!(g.notes.len(), 2);
        g.toggle_accent(2);
        assert!(g.notes[1].accent);
        g.toggle_step(2);
        assert_eq!(g.notes.len(), 1);
        g.set_steps_per_beat(2);
        assert_eq!(
            g.notes[0],
            GrooveNote {
                step: 0,
                len_steps: 1,
                accent: false
            }
        );
        assert_eq!(g.bar_notes(sig, 0)[0].len, PPQ / 2);
    }
}
