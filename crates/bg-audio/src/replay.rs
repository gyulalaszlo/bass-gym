//! Plays back part of a recording: the bass through [`Slot::Playback`], and the click and
//! chords re-scheduled from the sound log, shifted to the current transport time. Sounds
//! come from the voices selected now, at their logged times and gains.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use bg_core::plan::ClickKind;
use bg_sound::{SoundEvent, SoundKind};

use crate::recording::{LoggedSound, Recording};
use crate::session::{SessionIo, SessionLogic, SessionState, Slot};

/// Time between starting a replay and its first sound.
const START_LEAD: f64 = 0.2;
/// How far ahead sounds are scheduled.
const LOOKAHEAD: f64 = 0.35;
/// Extra time after the end, so the last block plays out.
const TAIL: f64 = 0.1;

/// Follows and steers a running replay from the UI.
#[derive(Clone)]
pub struct ReplayHandle {
    inner: Arc<Shared>,
}

struct Shared {
    /// Add to a recorded time to get its transport time now (f64 bits, NaN until known).
    shift: AtomicU64,
    click: AtomicBool,
    chords: AtomicBool,
}

impl ReplayHandle {
    pub fn new(click: bool, chords: bool) -> Self {
        Self {
            inner: Arc::new(Shared {
                shift: AtomicU64::new(f64::NAN.to_bits()),
                click: AtomicBool::new(click),
                chords: AtomicBool::new(chords),
            }),
        }
    }

    /// Plays the click from now on (after what's already scheduled).
    pub fn set_click(&self, on: bool) {
        self.inner.click.store(on, Ordering::Relaxed);
    }

    /// Plays the chords from now on (after what's already scheduled).
    pub fn set_chords(&self, on: bool) {
        self.inner.chords.store(on, Ordering::Relaxed);
    }

    /// The recorded time being heard, given the current transport time.
    pub fn position(&self, now: f64) -> Option<f64> {
        let shift = f64::from_bits(self.inner.shift.load(Ordering::Relaxed));
        (!shift.is_nan()).then_some(now - shift)
    }

    fn plays(&self, slot: Slot) -> bool {
        let flag = match slot {
            Slot::Click | Slot::Groove => &self.inner.click,
            Slot::Instrument => &self.inner.chords,
            Slot::Playback => return true,
        };
        flag.load(Ordering::Relaxed)
    }
}

/// The sounds of `from..to` to replay, in time order. Notes already sounding at `from`
/// start there with what's left of them, and notes are cut off at `to`. Sounds that never
/// reached the output are left out.
fn region_sounds(sounds: &[LoggedSound], from: f64, to: f64) -> Vec<LoggedSound> {
    let mut out: Vec<LoggedSound> = sounds
        .iter()
        .filter(|s| s.delivered && s.at < to)
        .filter_map(|s| {
            let mut s = *s;
            if let SoundKind::Note { duration, .. } = &mut s.ev.kind {
                let end = (s.at + *duration as f64).min(to);
                let start = s.at.max(from);
                if end <= start {
                    return None;
                }
                *duration = (end - start) as f32;
                s.at = start;
            } else if s.at < from {
                return None;
            }
            Some(s)
        })
        .collect();
    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    out
}

pub(crate) struct ReplaySession {
    sounds: Vec<LoggedSound>,
    next: usize,
    /// When and how loud to start the bass, if there's any in the region.
    bass: Option<(f64, f32)>,
    from: f64,
    to: f64,
    shift: f64,
    handle: ReplayHandle,
}

impl ReplaySession {
    /// `bass_from` is when the bass audio starts, and `bass_gain` its level.
    pub fn new(
        recording: &Recording,
        from: f64,
        to: f64,
        bass_from: Option<f64>,
        bass_gain: f32,
        handle: ReplayHandle,
    ) -> Self {
        Self {
            sounds: region_sounds(&recording.sounds, from, to),
            next: 0,
            bass: bass_from.map(|t| (t, bass_gain)),
            from,
            to,
            shift: 0.0,
            handle,
        }
    }
}

impl SessionLogic for ReplaySession {
    fn start(&mut self, io: &mut SessionIo) {
        self.shift = io.now + START_LEAD - self.from;
        self.handle
            .inner
            .shift
            .store(self.shift.to_bits(), Ordering::Relaxed);
        if let Some((at, gain)) = self.bass {
            let ev = SoundEvent {
                kind: SoundKind::Click(ClickKind::Beat),
                gain,
            };
            io.play(at + self.shift, Slot::Playback, ev);
        }
    }

    fn tick(&mut self, io: &mut SessionIo) -> SessionState {
        while let Some(s) = self.sounds.get(self.next) {
            let at = s.at + self.shift;
            if at > io.now + LOOKAHEAD {
                break;
            }
            if self.handle.plays(s.slot) {
                io.play(at, s.slot, s.ev);
            }
            self.next += 1;
        }
        if io.now > self.to + self.shift + TAIL {
            SessionState::Finished
        } else {
            SessionState::Running
        }
    }

    fn note_on(&mut self, _time: f64, _level_db: f32) {}

    fn note_off(&mut self, _time: f64, _peak_db: f32, _freq: Option<f32>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::Recorder;

    fn click(at: f64) -> LoggedSound {
        LoggedSound {
            at,
            slot: Slot::Click,
            ev: SoundEvent {
                kind: SoundKind::Click(ClickKind::Beat),
                gain: 0.7,
            },
            delivered: true,
        }
    }

    fn note(at: f64, duration: f32) -> LoggedSound {
        LoggedSound {
            at,
            slot: Slot::Instrument,
            ev: SoundEvent {
                kind: SoundKind::Note {
                    midi: 60.0,
                    velocity: 1.0,
                    duration,
                },
                gain: 0.5,
            },
            delivered: true,
        }
    }

    fn duration(s: &LoggedSound) -> f32 {
        match s.ev.kind {
            SoundKind::Note { duration, .. } => duration,
            _ => panic!("not a note"),
        }
    }

    fn recording(sounds: &[LoggedSound]) -> Recording {
        let mut r = Recorder::new(usize::MAX);
        r.sounds_mut().extend_from_slice(sounds);
        r.finish(100.0, 0.0)
    }

    /// Runs a replay from transport time `start`, ticking every 10 ms, and logs what it
    /// schedules.
    fn run(mut s: ReplaySession, start: f64) -> Vec<LoggedSound> {
        let mut log = Vec::new();
        s.start(&mut SessionIo::offline_logged(start, 0.0, &mut log));
        let mut now = start;
        while s.tick(&mut SessionIo::offline_logged(now, 0.0, &mut log)) == SessionState::Running {
            now += 0.01;
        }
        log
    }

    #[test]
    fn region_keeps_what_sounds_inside_it() {
        let sounds = [
            click(0.5),
            note(0.8, 0.4), // Still sounding at 1.0.
            note(0.2, 0.3), // Over by then.
            click(1.5),
            note(1.8, 1.0), // Cut at 2.0.
            click(2.5),
            LoggedSound {
                delivered: false,
                ..click(1.6)
            },
        ];
        let r = region_sounds(&sounds, 1.0, 2.0);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].at, 1.0);
        assert!((duration(&r[0]) - 0.2).abs() < 1e-6);
        assert_eq!(r[1].at, 1.5);
        assert_eq!(r[2].at, 1.8);
        assert!((duration(&r[2]) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn sounds_keep_their_spacing_and_gain() {
        let rec = recording(&[click(10.0), click(10.5), note(11.0, 0.25)]);
        let handle = ReplayHandle::new(true, true);
        let s = ReplaySession::new(&rec, 10.0, 12.0, None, 1.0, handle.clone());
        let log = run(s, 50.0);
        let times: Vec<f64> = log.iter().map(|s| s.at).collect();
        assert_eq!(times.len(), 3);
        assert!((times[0] - (50.0 + START_LEAD)).abs() < 1e-9);
        assert!((times[1] - times[0] - 0.5).abs() < 1e-9);
        assert!((times[2] - times[0] - 1.0).abs() < 1e-9);
        assert_eq!(log[0].ev.gain, 0.7);
        assert_eq!(log[2].ev.gain, 0.5);
        let position = handle.position(50.0 + START_LEAD + 0.5).unwrap();
        assert!((position - 10.5).abs() < 1e-9);
    }

    #[test]
    fn checkboxes_filter_by_slot() {
        let groove = LoggedSound {
            slot: Slot::Groove,
            ..click(1.1)
        };
        let rec = recording(&[click(1.0), groove, note(1.2, 0.2)]);
        let only = |click, chords| {
            let handle = ReplayHandle::new(click, chords);
            let s = ReplaySession::new(&rec, 1.0, 2.0, None, 1.0, handle);
            run(s, 0.0).iter().map(|s| s.slot).collect::<Vec<_>>()
        };
        assert_eq!(only(true, false), [Slot::Click, Slot::Groove]);
        assert_eq!(only(false, true), [Slot::Instrument]);
    }

    #[test]
    fn bass_starts_with_the_region() {
        let rec = recording(&[]);
        let handle = ReplayHandle::new(false, false);
        // The audio only begins half a second into the region.
        let s = ReplaySession::new(&rec, 1.0, 2.0, Some(1.5), 0.9, handle);
        let log = run(s, 0.0);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].slot, Slot::Playback);
        assert!((log[0].at - (START_LEAD + 0.5)).abs() < 1e-9);
        assert_eq!(log[0].ev.gain, 0.9);
    }

    #[test]
    fn position_is_unknown_until_started() {
        assert_eq!(ReplayHandle::new(true, true).position(5.0), None);
    }
}
