//! Bug reports: a region of a recorded session written out as audio, every marker and
//! event as JSON, and the click and chords as MIDI, in a new folder plus a zip of it.
//!
//! All report times are seconds from the start of `audio.wav` (and of the MIDI file),
//! found through the recording's time map, so they land on the audio they belong to.

mod midi;

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use bg_audio::{Recording, Slot};
use bg_core::music::freq_to_midi;
use bg_core::score::ScoreChord;
use bg_core::scoring::Grade;
use bg_core::{BarPlan, Matcher};
use bg_sound::SoundKind;
use serde::Serialize;
use serde_json::Value;

const AUDIO_FILE: &str = "audio.wav";
const JSON_FILE: &str = "report.json";
const MIDI_FILE: &str = "click_chords.mid";
/// Name of the folder reports go in, within the app's data folder.
pub const REPORTS_FOLDER: &str = "reports";
/// Bumped when the JSON layout changes.
const FORMAT_VERSION: u32 = 1;
/// Audio and events kept either side of the selected bars, in seconds.
pub const PADDING: f64 = 0.5;

/// A user annotation, in transport time.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub time: f64,
    pub text: String,
}

/// Everything a report is made from.
#[derive(Clone)]
pub struct ReportInput {
    pub title: String,
    pub notes: String,
    pub exercise: String,
    /// Settings in effect, as they should appear in the report.
    pub settings: Value,
    pub a4: f32,
    pub recording: Arc<Recording>,
    pub bars: Vec<BarPlan>,
    pub matcher: Matcher,
    pub chords: Vec<ScoreChord>,
    pub annotations: Vec<Annotation>,
    /// First and last selected bar, numbered from 1.
    pub first_bar: i64,
    pub last_bar: i64,
    /// Transport time of the selected bars.
    pub from: f64,
    pub to: f64,
}

/// Where a report ended up.
#[derive(Clone, Debug, PartialEq)]
pub struct Exported {
    pub folder: PathBuf,
    pub zip: PathBuf,
}

/// The stretch of the session that goes into a report.
pub(crate) struct Region<'a> {
    recording: &'a Recording,
    pub start: f64,
    pub end: f64,
    first_sample: u64,
    last_sample: u64,
}

impl<'a> Region<'a> {
    fn new(recording: &'a Recording, from: f64, to: f64) -> Self {
        let (mut start, mut end) = (from - PADDING, to + PADDING);
        if recording.has_audio() {
            start = start.max(recording.first_time());
            end = end.min(recording.end_time());
        }
        Self {
            recording,
            start,
            end,
            first_sample: recording.sample_at(start),
            last_sample: recording.sample_at(end),
        }
    }

    /// Seconds from the start of the audio file.
    pub fn rel(&self, t: f64) -> f64 {
        if self.recording.has_audio() {
            let sr = self.recording.audio.sample_rate() as f64;
            let at = self.recording.sample_at(t.max(self.start)) as f64;
            (at - self.first_sample as f64) / sr - (self.start - t).max(0.0)
        } else {
            t - self.start
        }
    }

    fn contains(&self, t: f64) -> bool {
        (self.start..=self.end).contains(&t)
    }

    fn overlaps(&self, from: f64, to: f64) -> bool {
        from <= self.end && to >= self.start
    }
}

#[derive(Serialize)]
struct Report {
    format_version: u32,
    title: String,
    notes: String,
    created: String,
    app_version: &'static str,
    exercise: String,
    selection: Selection,
    audio: AudioInfo,
    settings: Value,
    bars: Vec<BarEntry>,
    expected: Vec<ExpectedEntry>,
    played: Vec<PlayedEntry>,
    chords: Vec<ChordEntry>,
    sounds: Vec<SoundEntry>,
    annotations: Vec<AnnotationEntry>,
    gaps: Vec<GapEntry>,
}

#[derive(Serialize)]
struct Selection {
    first_bar: i64,
    last_bar: i64,
    /// Where the selected bars start and end in the audio file.
    start: f64,
    end: f64,
}

#[derive(Serialize)]
struct AudioInfo {
    file: Option<&'static str>,
    sample_rate: f32,
    seconds: f64,
    /// Transport time of the file's first sample.
    transport_start: f64,
    latency_s: f64,
}

#[derive(Serialize)]
struct BarEntry {
    bar: i64,
    start: f64,
    end: f64,
    bpm: f32,
    time_signature: String,
    steps_per_beat: u32,
    click_muted: bool,
}

#[derive(Serialize)]
struct ExpectedEntry {
    index: usize,
    bar: i64,
    time: f64,
    length: f64,
    accent: bool,
    window: f64,
    grade: Option<&'static str>,
    played: Option<usize>,
    timing_ms: Option<f32>,
    length_ratio: Option<f32>,
    harmony: Option<f32>,
    harmony_grade: Option<&'static str>,
}

#[derive(Serialize)]
struct PlayedEntry {
    index: usize,
    start: f64,
    end: Option<f64>,
    peak_db: f32,
    frequency: Option<f32>,
    midi: Option<i32>,
    cents: Option<f32>,
    harmony: Option<f32>,
    expected: Option<usize>,
}

#[derive(Serialize)]
struct ChordEntry {
    start: f64,
    end: f64,
    label: String,
    tones: Vec<u8>,
}

#[derive(Serialize)]
struct SoundEntry {
    time: f64,
    slot: &'static str,
    kind: &'static str,
    midi: Option<f32>,
    velocity: Option<f32>,
    duration: Option<f32>,
    gain: f32,
    delivered: bool,
}

#[derive(Serialize)]
struct AnnotationEntry {
    time: f64,
    text: String,
}

#[derive(Serialize)]
struct GapEntry {
    time: f64,
    seconds: f64,
}

fn grade_name(g: Grade) -> &'static str {
    match g {
        Grade::Good => "good",
        Grade::Ok => "ok",
        Grade::Poor => "poor",
        Grade::Missed => "missed",
    }
}

pub(crate) fn slot_name(s: Slot) -> &'static str {
    match s {
        Slot::Click => "click",
        Slot::Groove => "groove",
        Slot::Instrument => "instrument",
        Slot::Playback => "playback",
    }
}

fn report(input: &ReportInput, region: &Region) -> Report {
    let rec = &input.recording;
    let m = &input.matcher;
    let rel = |t| region.rel(t);
    let bars = input
        .bars
        .iter()
        .filter(|b| !b.is_count_in() && region.overlaps(b.start, b.end()))
        .map(|b| BarEntry {
            bar: b.index + 1,
            start: rel(b.start),
            end: rel(b.end()),
            bpm: b.bpm,
            time_signature: format!("{}/{}", b.sig.num, b.sig.den),
            steps_per_beat: b.steps_per_beat,
            click_muted: b.muted,
        })
        .collect();
    let expected = m
        .expected
        .iter()
        .enumerate()
        .filter(|(_, e)| region.contains(e.time))
        .map(|(index, e)| {
            let o = m.outcomes.get(index).cloned().unwrap_or_default();
            ExpectedEntry {
                index,
                bar: e.bar + 1,
                time: rel(e.time),
                length: e.len,
                accent: e.accent,
                window: e.window,
                grade: o.grade.map(grade_name),
                played: o.played,
                timing_ms: o.timing_ms,
                length_ratio: o.length_ratio,
                harmony: o.harmony,
                harmony_grade: o.harmony_grade.map(grade_name),
            }
        })
        .collect();
    let played = m
        .played
        .iter()
        .enumerate()
        .filter(|(_, p)| region.overlaps(p.onset, p.offset.unwrap_or(p.onset)))
        .map(|(index, p)| {
            let pitch = p.freq.map(|f| freq_to_midi(f, input.a4));
            PlayedEntry {
                index,
                start: rel(p.onset),
                end: p.offset.map(rel),
                peak_db: p.peak_db,
                frequency: p.freq,
                midi: pitch.map(|x| x.0),
                cents: pitch.map(|x| x.1),
                harmony: p.harmony,
                expected: p.matched,
            }
        })
        .collect();
    let chords = input
        .chords
        .iter()
        .filter(|c| region.overlaps(c.start, c.end))
        .map(|c| ChordEntry {
            start: rel(c.start),
            end: rel(c.end),
            label: c.label.clone(),
            tones: c.tones.clone(),
        })
        .collect();
    let sounds = rec
        .sounds
        .iter()
        .filter(|s| region.contains(s.at))
        .map(|s| {
            let (kind, midi, velocity, duration) = match s.ev.kind {
                SoundKind::Click(k) => (click_name(k), None, None, None),
                SoundKind::Note {
                    midi,
                    velocity,
                    duration,
                } => ("note", Some(midi), Some(velocity), Some(duration)),
            };
            SoundEntry {
                time: rel(s.at),
                slot: slot_name(s.slot),
                kind,
                midi,
                velocity,
                duration,
                gain: s.ev.gain,
                delivered: s.delivered,
            }
        })
        .collect();
    let annotations = input
        .annotations
        .iter()
        .filter(|a| region.contains(a.time))
        .map(|a| AnnotationEntry {
            time: rel(a.time),
            text: a.text.clone(),
        })
        .collect();
    let gaps = rec
        .gaps
        .iter()
        .filter(|g| region.overlaps(g.time, g.time + g.seconds))
        .map(|g| GapEntry {
            time: rel(g.time),
            seconds: g.seconds,
        })
        .collect();
    let sample_rate = rec.audio.sample_rate();
    let samples = region.last_sample.saturating_sub(region.first_sample);
    Report {
        format_version: FORMAT_VERSION,
        title: input.title.clone(),
        notes: input.notes.clone(),
        created: chrono::Local::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION"),
        exercise: input.exercise.clone(),
        selection: Selection {
            first_bar: input.first_bar,
            last_bar: input.last_bar,
            start: rel(input.from),
            end: rel(input.to),
        },
        audio: AudioInfo {
            file: rec.has_audio().then_some(AUDIO_FILE),
            sample_rate,
            seconds: samples as f64 / sample_rate.max(1.0) as f64,
            transport_start: region.start,
            latency_s: rec.latency,
        },
        settings: input.settings.clone(),
        bars,
        expected,
        played,
        chords,
        sounds,
        annotations,
        gaps,
    }
}

fn click_name(k: bg_core::ClickKind) -> &'static str {
    match k {
        bg_core::ClickKind::Accent => "accent",
        bg_core::ClickKind::Beat => "beat",
        bg_core::ClickKind::Sub => "sub",
    }
}

fn write_audio(path: &Path, input: &ReportInput, region: &Region) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: input.recording.audio.sample_rate() as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(path, spec)?;
    let audio = &input.recording.audio;
    for s in audio.copy_range(region.first_sample, region.last_sample) {
        wav.write_sample(s)?;
    }
    wav.finalize()?;
    Ok(())
}

/// A folder name not yet taken in `parent`.
fn new_folder(parent: &Path, name: &str) -> PathBuf {
    let free = |p: &PathBuf| !p.exists() && !p.with_extension("zip").exists();
    let first = parent.join(name);
    if free(&first) {
        return first;
    }
    (2..)
        .map(|n| parent.join(format!("{name}-{n}")))
        .find(free)
        .unwrap_or(first)
}

fn zip_folder(folder: &Path, zip_path: &Path) -> Result<()> {
    let mut zip = zip::ZipWriter::new(File::create(zip_path)?);
    let options = zip::write::SimpleFileOptions::default();
    for file in [AUDIO_FILE, JSON_FILE, MIDI_FILE] {
        let path = folder.join(file);
        if !path.exists() {
            continue;
        }
        zip.start_file(file, options)?;
        let mut data = Vec::new();
        File::open(&path)?.read_to_end(&mut data)?;
        zip.write_all(&data)?;
    }
    zip.finish()?;
    Ok(())
}

/// Whether a folder or zip is one of ours: `2026-11-07_145046_bars-31-32`, optionally with
/// a counter and `.zip`.
fn is_report(name: &str) -> bool {
    let (date, rest) = name.split_at_checked(10).unwrap_or(("", ""));
    let digits = date.chars().filter(char::is_ascii_digit).count();
    digits == 8
        && date.matches('-').count() == 2
        && rest.starts_with('_')
        && rest.contains("_bars-")
}

fn size_of(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path).map_or(0, |entries| {
        entries.flatten().map(|e| size_of(&e.path())).sum()
    })
}

/// The reports in a folder, and what they take up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReportsSummary {
    pub count: usize,
    pub bytes: u64,
}

fn report_paths(reports_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(reports_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(is_report))
        .map(|e| e.path())
        .collect()
}

pub fn summary(reports_dir: &Path) -> ReportsSummary {
    let paths = report_paths(reports_dir);
    let mut names: Vec<_> = paths.iter().filter_map(|p| p.file_stem()).collect();
    names.sort();
    names.dedup();
    ReportsSummary {
        count: names.len(),
        bytes: paths.iter().map(|p| size_of(p)).sum(),
    }
}

/// Deletes every report in `reports_dir`, leaving anything else there alone.
pub fn clear(reports_dir: &Path) -> Result<()> {
    for path in report_paths(reports_dir) {
        let removed = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        removed.with_context(|| format!("deleting {}", path.display()))?;
    }
    Ok(())
}

/// Writes a report into a new folder in `reports_dir`, with a zip of it alongside.
pub fn export(input: &ReportInput, reports_dir: &Path) -> Result<Exported> {
    fs::create_dir_all(reports_dir)
        .with_context(|| format!("creating {}", reports_dir.display()))?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
    let name = format!("{stamp}_bars-{}-{}", input.first_bar, input.last_bar);
    let folder = new_folder(reports_dir, &name);
    fs::create_dir(&folder).with_context(|| format!("creating {}", folder.display()))?;

    let region = Region::new(&input.recording, input.from, input.to);
    if input.recording.has_audio() {
        write_audio(&folder.join(AUDIO_FILE), input, &region).context("writing the audio")?;
    }
    let json = serde_json::to_vec_pretty(&report(input, &region))?;
    fs::write(folder.join(JSON_FILE), json).context("writing the report")?;
    fs::write(
        folder.join(MIDI_FILE),
        midi::render(&input.recording, &region)?,
    )
    .context("writing the MIDI file")?;

    let zip = folder.with_extension("zip");
    zip_folder(&folder, &zip).context("zipping the report")?;
    Ok(Exported { folder, zip })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_audio::{LoggedSound, Recorder};
    use bg_core::ClickKind;
    use bg_core::plan::PlannedNote;
    use bg_core::{ScoringThresholds, TimeSignature};
    use bg_sound::SoundEvent;

    const SR: f32 = 1000.0;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bg-report-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn input() -> ReportInput {
        // Six seconds of audio from transport time 100, numbered by sample.
        let mut r = Recorder::new(usize::MAX);
        let audio: Vec<f32> = (0..6000).map(|i| i as f32 / 6000.0).collect();
        r.feed(&audio, SR, 0, Some(100.0));
        let sound = |at, slot, kind| LoggedSound {
            at,
            slot,
            ev: SoundEvent { kind, gain: 0.5 },
            delivered: true,
        };
        r.sounds_mut().extend([
            sound(100.2, Slot::Click, SoundKind::Click(ClickKind::Accent)), // Outside.
            sound(102.0, Slot::Click, SoundKind::Click(ClickKind::Accent)),
            sound(
                102.5,
                Slot::Instrument,
                SoundKind::Note {
                    midi: 60.0,
                    velocity: 0.8,
                    duration: 0.5,
                },
            ),
        ]);
        let recording = Arc::new(r.finish(106.0, 0.012));
        let bar = |index: i64, start: f64| BarPlan {
            index,
            start,
            duration: 2.0,
            bpm: 120.0,
            beats: 4,
            sig: TimeSignature::default(),
            steps_per_beat: 1,
            muted: false,
            clicks: Vec::new(),
            notes: vec![PlannedNote {
                time: start,
                len: 0.5,
                accent: true,
            }],
        };
        let bars = vec![bar(0, 100.0), bar(1, 102.0), bar(2, 104.0)];
        let mut matcher = Matcher::new(ScoringThresholds::default());
        bars.iter().for_each(|b| matcher.add_bar(b));
        let note = matcher.on_onset(102.01, -9.0);
        matcher.on_offset(note, 102.4, -9.0, Some(82.41));
        ReportInput {
            title: "Late note".into(),
            notes: "Marked by hand".into(),
            exercise: "basic_timing".into(),
            settings: serde_json::json!({ "tempo": 120 }),
            a4: 440.0,
            recording,
            bars,
            matcher,
            chords: Vec::new(),
            annotations: vec![
                Annotation {
                    time: 102.3,
                    text: "note end should be here".into(),
                },
                Annotation {
                    time: 100.1,
                    text: "outside".into(),
                },
            ],
            first_bar: 2,
            last_bar: 2,
            from: 102.0,
            to: 104.0,
        }
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    #[test]
    fn region_is_padded_and_clamped_to_the_audio() {
        let i = input();
        let r = Region::new(&i.recording, 102.0, 104.0);
        close(r.start, 101.5);
        close(r.end, 104.5);
        close(r.rel(102.0), 0.5);
        let edge = Region::new(&i.recording, 100.0, 106.0);
        close(edge.start, 100.0);
        close(edge.end, 106.0);
    }

    #[test]
    fn exports_audio_events_and_midi_that_agree() {
        let dir = scratch("round-trip");
        let i = input();
        let out = export(&i, &dir).unwrap();
        assert!(out.folder.starts_with(&dir));

        // Audio: the padded region, starting on the right sample.
        let mut wav = hound::WavReader::open(out.folder.join(AUDIO_FILE)).unwrap();
        assert_eq!(wav.spec().sample_rate, 1000);
        assert_eq!(wav.duration(), 3000);
        let first: f32 = wav.samples::<f32>().next().unwrap().unwrap();
        assert!((first - 1500.0 / 6000.0).abs() < 1e-6);

        // Report: times are relative to that audio; outside events are left out.
        let json = fs::read(out.folder.join(JSON_FILE)).unwrap();
        let r: Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(r["title"], "Late note");
        assert_eq!(r["settings"]["tempo"], 120);
        close(r["selection"]["start"].as_f64().unwrap(), 0.5);
        close(r["audio"]["transport_start"].as_f64().unwrap(), 101.5);
        close(r["audio"]["seconds"].as_f64().unwrap(), 3.0);
        let bars = r["bars"].as_array().unwrap();
        assert_eq!(bars.len(), 3); // Bars 1 and 3 reach into the padding.
        assert_eq!(r["played"][0]["midi"], 40);
        close(r["played"][0]["start"].as_f64().unwrap(), 0.51);
        assert_eq!(r["annotations"].as_array().unwrap().len(), 1);
        close(r["annotations"][0]["time"].as_f64().unwrap(), 0.8);
        assert_eq!(r["sounds"].as_array().unwrap().len(), 2);
        close(r["sounds"][1]["time"].as_f64().unwrap(), 1.0);

        // MIDI: the same click and chord times.
        let data = fs::read(out.folder.join(MIDI_FILE)).unwrap();
        let notes = midi::note_ons(&data);
        assert_eq!(notes.len(), 2);
        close((notes[0].1 * 1000.0).round() / 1000.0, 0.5);
        close((notes[1].1 * 1000.0).round() / 1000.0, 1.0);
        assert_eq!(notes[1].0, 60);

        // Zip: the same three files.
        let mut zip = zip::ZipArchive::new(File::open(&out.zip).unwrap()).unwrap();
        assert_eq!(zip.len(), 3);
        let mut stored = Vec::new();
        zip.by_name(JSON_FILE)
            .unwrap()
            .read_to_end(&mut stored)
            .unwrap();
        assert_eq!(stored, json);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_report_without_audio_still_has_its_events() {
        let dir = scratch("no-audio");
        let mut i = input();
        let mut r = Recorder::new(usize::MAX);
        r.sounds_mut().extend(i.recording.sounds.iter().copied());
        i.recording = Arc::new(r.finish(106.0, 0.0));
        let out = export(&i, &dir).unwrap();
        assert!(!out.folder.join(AUDIO_FILE).exists());
        let r: Value =
            serde_json::from_slice(&fs::read(out.folder.join(JSON_FILE)).unwrap()).unwrap();
        assert_eq!(r["audio"]["file"], Value::Null);
        close(r["played"][0]["start"].as_f64().unwrap(), 0.51);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_report_names_count_as_reports() {
        assert!(is_report("2026-11-07_145046_bars-31-32"));
        assert!(is_report("2026-11-07_145046_bars-31-32-2"));
        assert!(is_report("2026-11-07_145046_bars-3-3.zip"));
        assert!(!is_report(".DS_Store"));
        assert!(!is_report("my notes"));
        assert!(!is_report("bars-3-3"));
        assert!(!is_report("2026-11-07_notes.txt"));
    }

    #[test]
    fn clearing_removes_reports_and_nothing_else() {
        let dir = scratch("clear");
        export(&input(), &dir).unwrap();
        export(&input(), &dir).unwrap();
        fs::write(dir.join("keep me.txt"), "mine").unwrap();
        fs::create_dir(dir.join("my folder")).unwrap();

        let before = summary(&dir);
        assert_eq!(before.count, 2);
        assert!(before.bytes > 2 * 24_000); // Two copies of 3000 float samples, zipped too.

        clear(&dir).unwrap();
        assert_eq!(summary(&dir), ReportsSummary::default());
        assert!(dir.join("keep me.txt").exists());
        assert!(dir.join("my folder").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_folder_is_an_empty_one() {
        let dir = scratch("missing");
        assert_eq!(summary(&dir), ReportsSummary::default());
        clear(&dir).unwrap();
    }

    #[test]
    fn folder_names_never_collide() {
        let dir = scratch("names");
        fs::create_dir_all(dir.join("report")).unwrap();
        assert_eq!(new_folder(&dir, "report"), dir.join("report-2"));
        assert_eq!(new_folder(&dir, "other"), dir.join("other"));
        fs::remove_dir_all(dir).unwrap();
    }
}
