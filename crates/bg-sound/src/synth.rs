//! Built-in synthesized voices. Each voice has a fixed number of slots, so triggering
//! never allocates.

use std::f32::consts::TAU;

use bg_core::music::midi_to_freq;
use bg_core::plan::ClickKind;

use crate::{SoundEvent, SoundKind, Voice};

const SLOTS: usize = 8;

#[derive(Clone, Copy, Default)]
struct Ping {
    phase: f32,
    inc: f32,
    amp: f32,
    decay: f32,
}

/// Short, bright sine blips: higher pitch for the accent.
pub struct SineClick {
    sr: f32,
    pings: [Ping; SLOTS],
    next: usize,
}

impl SineClick {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            pings: [Ping::default(); SLOTS],
            next: 0,
        }
    }
}

impl Voice for SineClick {
    fn trigger(&mut self, ev: &SoundEvent) {
        let (freq, level) = match ev.kind {
            SoundKind::Click(ClickKind::Accent) => (2000.0, 1.0),
            SoundKind::Click(ClickKind::Beat) => (1500.0, 0.7),
            SoundKind::Click(ClickKind::Sub) => (1000.0, 0.4),
            SoundKind::Note { midi, velocity, .. } => (midi_to_freq(midi + 36.0, 440.0), velocity),
        };
        self.pings[self.next] = Ping {
            phase: 0.0,
            inc: freq / self.sr,
            amp: level * ev.gain,
            decay: (-1.0 / (0.012 * self.sr)).exp(),
        };
        self.next = (self.next + 1) % SLOTS;
    }

    fn render(&mut self, out: &mut [f32]) {
        for p in self.pings.iter_mut().filter(|p| p.amp > 1e-5) {
            for o in out.iter_mut() {
                *o += (p.phase * TAU).sin() * p.amp;
                p.phase = (p.phase + p.inc).fract();
                p.amp *= p.decay;
            }
        }
    }

    fn silence(&mut self) {
        self.pings = [Ping::default(); SLOTS];
    }
}

/// Two damped resonators excited together, giving a hollow woody "tok".
pub struct Woodblock {
    sr: f32,
    pings: [[Ping; 2]; SLOTS],
    next: usize,
}

impl Woodblock {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            pings: [[Ping::default(); 2]; SLOTS],
            next: 0,
        }
    }
}

impl Voice for Woodblock {
    fn trigger(&mut self, ev: &SoundEvent) {
        let (base, level) = match ev.kind {
            SoundKind::Click(ClickKind::Accent) => (1250.0, 1.0),
            SoundKind::Click(ClickKind::Beat) => (950.0, 0.75),
            SoundKind::Click(ClickKind::Sub) => (800.0, 0.45),
            SoundKind::Note { midi, velocity, .. } => (midi_to_freq(midi + 24.0, 440.0), velocity),
        };
        let amp = level * ev.gain;
        self.pings[self.next] = [
            Ping {
                phase: 0.0,
                inc: base / self.sr,
                amp: amp * 0.65,
                decay: (-1.0 / (0.025 * self.sr)).exp(),
            },
            Ping {
                phase: 0.0,
                inc: base * 2.76 / self.sr,
                amp: amp * 0.3,
                decay: (-1.0 / (0.008 * self.sr)).exp(),
            },
        ];
        self.next = (self.next + 1) % SLOTS;
    }

    fn render(&mut self, out: &mut [f32]) {
        for p in self.pings.iter_mut().flatten().filter(|p| p.amp > 1e-5) {
            for o in out.iter_mut() {
                *o += (p.phase * TAU).sin() * p.amp;
                p.phase = (p.phase + p.inc).fract();
                p.amp *= p.decay;
            }
        }
    }

    fn silence(&mut self) {
        self.pings = [[Ping::default(); 2]; SLOTS];
    }
}

#[derive(Clone, Copy, Default)]
struct Pluck {
    phase: f32,
    inc: f32,
    amp: f32,
    decay: f32,
    remaining: u32,
    release: f32,
    lp: f32,
    cutoff: f32,
    cutoff_decay: f32,
}

/// Low-passed sawtooth pluck; also usable as a pitched "groove click".
pub struct PluckBass {
    sr: f32,
    notes: [Pluck; SLOTS],
    next: usize,
}

impl PluckBass {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            notes: [Pluck::default(); SLOTS],
            next: 0,
        }
    }
}

impl Voice for PluckBass {
    fn trigger(&mut self, ev: &SoundEvent) {
        let (midi, vel, dur) = match ev.kind {
            SoundKind::Note {
                midi,
                velocity,
                duration,
                ..
            } => (midi, velocity, duration),
            SoundKind::Click(ClickKind::Accent) => (45.0, 1.0, 0.12),
            SoundKind::Click(ClickKind::Beat) => (40.0, 0.8, 0.12),
            SoundKind::Click(ClickKind::Sub) => (40.0, 0.5, 0.08),
        };
        self.notes[self.next] = Pluck {
            phase: 0.0,
            inc: midi_to_freq(midi, 440.0) / self.sr,
            amp: vel * ev.gain * 0.6,
            decay: (-1.0 / (1.5 * self.sr)).exp(),
            remaining: (dur.max(0.02) * self.sr) as u32,
            release: (-1.0 / (0.015 * self.sr)).exp(),
            lp: 0.0,
            cutoff: 0.35,
            cutoff_decay: (-1.0 / (0.15 * self.sr)).exp(),
        };
        self.next = (self.next + 1) % SLOTS;
    }

    fn render(&mut self, out: &mut [f32]) {
        for n in self.notes.iter_mut().filter(|n| n.amp > 1e-5) {
            for o in out.iter_mut() {
                let saw = 2.0 * n.phase - 1.0;
                let c = 0.02 + n.cutoff;
                n.lp += c * (saw - n.lp);
                *o += n.lp * n.amp;
                n.phase = (n.phase + n.inc).fract();
                n.cutoff *= n.cutoff_decay;
                if n.remaining > 0 {
                    n.remaining -= 1;
                    n.amp *= n.decay;
                } else {
                    n.amp *= n.release;
                }
            }
        }
    }

    fn silence(&mut self) {
        self.notes = [Pluck::default(); SLOTS];
    }
}

#[derive(Clone, Copy, Default)]
struct FmNote {
    car: f32,
    modu: f32,
    inc: f32,
    amp: f32,
    index: f32,
    decay: f32,
    index_decay: f32,
    remaining: u32,
    release: f32,
}

/// Two-operator FM electric piano (tine-like), polyphonic for chords.
pub struct FmPiano {
    sr: f32,
    notes: [FmNote; 16],
    next: usize,
}

impl FmPiano {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            notes: [FmNote::default(); 16],
            next: 0,
        }
    }
}

impl Voice for FmPiano {
    fn trigger(&mut self, ev: &SoundEvent) {
        let (midi, vel, dur) = match ev.kind {
            SoundKind::Note {
                midi,
                velocity,
                duration,
                ..
            } => (midi, velocity, duration),
            SoundKind::Click(_) => (72.0, 0.6, 0.1),
        };
        self.notes[self.next] = FmNote {
            car: 0.0,
            modu: 0.0,
            inc: midi_to_freq(midi, 440.0) / self.sr,
            amp: vel * ev.gain * 0.25,
            index: 1.5 + vel,
            decay: (-1.0 / (2.0 * self.sr)).exp(),
            index_decay: (-1.0 / (0.4 * self.sr)).exp(),
            remaining: (dur.max(0.05) * self.sr) as u32,
            release: (-1.0 / (0.08 * self.sr)).exp(),
        };
        self.next = (self.next + 1) % self.notes.len();
    }

    fn render(&mut self, out: &mut [f32]) {
        for n in self.notes.iter_mut().filter(|n| n.amp > 1e-5) {
            for o in out.iter_mut() {
                let m = (n.modu * TAU).sin() * n.index;
                *o += (n.car * TAU + m).sin() * n.amp;
                n.car = (n.car + n.inc).fract();
                n.modu = (n.modu + n.inc * 14.0).fract();
                n.index = n.index * n.index_decay + 0.2 * (1.0 - n.index_decay);
                if n.remaining > 0 {
                    n.remaining -= 1;
                    n.amp *= n.decay;
                } else {
                    n.amp *= n.release;
                }
            }
        }
    }

    fn silence(&mut self) {
        self.notes = [FmNote::default(); 16];
    }
}
