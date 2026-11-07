//! The click and chords of a report as a standard MIDI file. One fixed tempo at a high
//! resolution keeps event times exact, whatever the session's tempo did: bars won't line
//! up with the file's grid, but every note lands where it played. Real tempos are in the
//! JSON.

use anyhow::{Result, anyhow};
use bg_audio::{LoggedSound, Recording, Slot};
use bg_core::ClickKind;
use bg_sound::SoundKind;
use midly::num::{u4, u7, u15, u24, u28};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

use crate::{Region, slot_name};

const TICKS_PER_BEAT: u16 = 9600;
/// 120 BPM.
const MICROS_PER_BEAT: u32 = 500_000;
const TICKS_PER_SECOND: f64 = TICKS_PER_BEAT as f64 * 1_000_000.0 / MICROS_PER_BEAT as f64;
const CLICK_SECONDS: f64 = 0.05;
const DRUM_CHANNEL: u8 = 9;
/// General MIDI percussion keys.
const KEY_HIGH_WOODBLOCK: u8 = 76;
const KEY_LOW_WOODBLOCK: u8 = 77;
const KEY_SIDE_STICK: u8 = 37;

struct Note {
    start: f64,
    end: f64,
    key: u8,
    velocity: u8,
}

fn velocity(v: f32) -> u8 {
    (v * 127.0).round().clamp(1.0, 127.0) as u8
}

/// A logged sound as a note in the file's time, cut to the region.
fn note(s: &LoggedSound, region: &Region) -> Option<Note> {
    let (key, seconds, vel) = match (s.slot, s.ev.kind) {
        (_, SoundKind::Click(k)) => {
            let key = match k {
                ClickKind::Accent => KEY_HIGH_WOODBLOCK,
                _ => KEY_LOW_WOODBLOCK,
            };
            (key, CLICK_SECONDS, velocity(s.ev.gain))
        }
        (Slot::Groove, SoundKind::Note { duration, .. }) => {
            (KEY_SIDE_STICK, duration as f64, velocity(s.ev.gain))
        }
        (
            _,
            SoundKind::Note {
                midi,
                velocity: v,
                duration,
            },
        ) => (
            midi.round().clamp(0.0, 127.0) as u8,
            duration as f64,
            velocity(v),
        ),
    };
    let (start, end) = (s.at.max(region.start), (s.at + seconds).min(region.end));
    (s.delivered && start < end).then(|| Note {
        start: region.rel(start),
        end: region.rel(end),
        key,
        velocity: vel,
    })
}

fn ticks(t: f64) -> u32 {
    (t.max(0.0) * TICKS_PER_SECOND).round() as u32
}

fn track<'a>(name: &'a str, channel: u8, notes: &[Note]) -> Vec<TrackEvent<'a>> {
    let mut timed: Vec<(u32, bool, MidiMessage)> = Vec::new();
    for n in notes {
        let key = u7::new(n.key);
        let vel = u7::new(n.velocity);
        timed.push((ticks(n.start), true, MidiMessage::NoteOn { key, vel }));
        timed.push((
            ticks(n.end),
            false,
            MidiMessage::NoteOff {
                key,
                vel: u7::new(0),
            },
        ));
    }
    // Note-offs first, so a repeated note isn't cut by the previous one's end.
    timed.sort_by_key(|(tick, on, _)| (*tick, *on));
    let mut events = vec![TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::TrackName(name.as_bytes())),
    }];
    let mut last = 0;
    for (tick, _, message) in timed {
        events.push(TrackEvent {
            delta: u28::new(tick - last),
            kind: TrackEventKind::Midi {
                channel: u4::new(channel),
                message,
            },
        });
        last = tick;
    }
    events.push(end_of_track());
    events
}

fn end_of_track<'a>() -> TrackEvent<'a> {
    TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    }
}

pub(crate) fn render(recording: &Recording, region: &Region) -> Result<Vec<u8>> {
    let notes_of = |slot: Slot| -> Vec<Note> {
        recording
            .sounds
            .iter()
            .filter(|s| s.slot == slot)
            .filter_map(|s| note(s, region))
            .collect()
    };
    let conductor = vec![
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(MICROS_PER_BEAT))),
        },
        end_of_track(),
    ];
    let mut smf = Smf::new(Header::new(
        Format::Parallel,
        Timing::Metrical(u15::new(TICKS_PER_BEAT)),
    ));
    smf.tracks.push(conductor);
    smf.tracks.push(track(
        slot_name(Slot::Click),
        DRUM_CHANNEL,
        &notes_of(Slot::Click),
    ));
    smf.tracks.push(track(
        slot_name(Slot::Groove),
        DRUM_CHANNEL,
        &notes_of(Slot::Groove),
    ));
    smf.tracks
        .push(track("chords", 0, &notes_of(Slot::Instrument)));
    let mut out = Vec::new();
    smf.write_std(&mut out)
        .map_err(|e| anyhow!("writing MIDI: {e}"))?;
    Ok(out)
}

/// Keys and times (seconds) of every note-on in a file, in time order.
#[cfg(test)]
pub(crate) fn note_ons(data: &[u8]) -> Vec<(u8, f64)> {
    let smf = Smf::parse(data).unwrap();
    let mut out = Vec::new();
    for track in &smf.tracks {
        let mut tick = 0u32;
        for e in track {
            tick += e.delta.as_int();
            if let TrackEventKind::Midi {
                message: MidiMessage::NoteOn { key, .. },
                ..
            } = e.kind
            {
                out.push((key.as_int(), tick as f64 / TICKS_PER_SECOND));
            }
        }
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}
