//! The Score + Audio tab of the results: what was played as a score over the recorded
//! waveform, with playback and a marker editor. State lives here, on the exercise, so it
//! outlasts the dialog; it's released with the session's results.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use bg_audio::{AnalysisConfig, AudioConfig, AudioEngine, Recording, ReplayHandle, SoundConfig};
use bg_core::harmony::Scale;
use bg_core::score::{self, ScoreBar, ScoreChord, ScoreEvent, ScoreInput, ScoreKind};
use bg_core::{BarPlan, Matcher};
use bg_report::{Annotation, Exported, ReportInput};
use egui::{
    Align2, Color32, Key, Modifiers, Rangef, Rect, RichText, Sense, Stroke, StrokeKind, vec2,
};

use crate::exercise::ExerciseCtx;
use crate::folder::open_folder;
use crate::icons;
use crate::settings::{ANALYSIS, AUDIO, SOUNDS};
use crate::theme::{self, mono};
use crate::widgets::button::button;
use crate::widgets::controls::{checkbox, number_field, text_field};
use crate::widgets::marker_editor::{
    EditorState, Header, Line, Marker, MarkerEditor, WaveView, paint_marker,
};
use crate::widgets::score::{Lead, ScoreView, StaffLayout, lead_width};
use crate::widgets::time_axis::TimeAxis;
use crate::widgets::waveform::{self, PeakSource, WaveStyle};

const STAFF_SPACING: f32 = 8.0;
const WAVE_HEIGHT: f32 = 60.0;
/// Room for the note bars above the waveform.
const NOTE_LANE: f32 = 10.0;
const ROW_GAP: f32 = 18.0;
const EDITOR_HEIGHT: f32 = 260.0;
/// Seconds shown either side of a click when opening the marker editor.
const EDITOR_SPAN: f64 = 1.0;
const BASS_GAIN: f32 = 1.0;
/// How close to a note a click on the score counts as clicking it, in points.
const NOTE_REACH: f32 = 12.0;
/// Playback from a note starts this long before it, so its attack is heard whole.
const NOTE_LEAD: f64 = 0.02;

/// What a finished session leaves behind for review.
pub struct ReviewInput {
    /// Count-in included.
    pub bars: Vec<BarPlan>,
    pub matcher: Matcher,
    pub chords: Vec<ScoreChord>,
    pub scale: Option<Scale>,
    pub a4: f32,
    pub recording: Arc<Recording>,
    /// The exercise's id.
    pub exercise: String,
    /// Settings to put in reports; see [`report_settings`].
    pub settings: serde_json::Value,
}

/// The settings a report should record: the exercise's own, and those of the app.
pub fn report_settings(ctx: &ExerciseCtx, exercise: serde_json::Value) -> serde_json::Value {
    let status = ctx.engine.status();
    serde_json::json!({
        "exercise": exercise,
        "scoring": ctx.scoring,
        "analysis": ctx.settings.get::<AnalysisConfig>(ANALYSIS),
        "audio": ctx.settings.get::<AudioConfig>(AUDIO),
        "sounds": ctx.settings.get::<SoundConfig>(SOUNDS),
        "devices": {
            "input": status.input,
            "output": status.output,
            "input_rate": status.input_rate,
            "output_rate": status.output_rate,
        },
    })
}

/// How the latest export went.
enum ExportState {
    Idle,
    Running(Receiver<Result<Exported, String>>),
    Done { folder: PathBuf, copied: bool },
    Failed(String),
}

/// A recording as a [`PeakSource`] on the transport timeline.
struct RecordingPeaks<'a>(&'a Recording);

impl PeakSource for RecordingPeaks<'_> {
    fn range(&self) -> (f64, f64) {
        (self.0.first_time(), self.0.end_time())
    }

    fn peak(&self, from: f64, to: f64) -> Option<(f32, f32)> {
        self.0
            .audio
            .peak_range(self.0.sample_at(from), self.0.sample_at(to))
    }
}

/// A position in the music: bar (from 1), beat (from 1) and 32nd note within the beat
/// (from 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicPosition {
    pub bar: i64,
    pub beat: u32,
    pub sub: u32,
}

/// The position of transport time `t` within `bars`.
pub fn position_at(bars: &[ScoreBar], t: f64) -> Option<MusicPosition> {
    let bar = bars.iter().rev().find(|b| b.start <= t).or(bars.first())?;
    let beats = bar.sig.num.max(1);
    let beat_len = (bar.end - bar.start) / beats as f64;
    let into = (t - bar.start).max(0.0);
    let beat = ((into / beat_len) as u32).min(beats - 1);
    let subs = (32 / bar.sig.den.max(1)).max(1);
    let sub = (((into - beat as f64 * beat_len) / beat_len * subs as f64) as u32).min(subs - 1);
    Some(MusicPosition {
        bar: bar.index + 1,
        beat: beat + 1,
        sub: sub + 1,
    })
}

/// Where playback should start for a click at `x` in `bar`: at the recorded note drawn
/// there, or else at the start of the bar.
fn click_start(bar: &ScoreBar, m: &Matcher, axis: TimeAxis, x: f32) -> f64 {
    let distance = |e: &ScoreEvent| (axis.x(e.time) - x).abs();
    bar.events
        .iter()
        .filter(|e| distance(e) <= NOTE_REACH)
        .filter_map(|e| match &e.kind {
            ScoreKind::Note(n) => Some((distance(e), m.played.get(n.played)?.onset)),
            _ => None,
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(bar.start, |(_, onset)| onset - NOTE_LEAD)
}

/// The expected, played and bar markers of a session.
fn overlays(m: &Matcher, bars: &[ScoreBar]) -> Vec<Marker> {
    let mut id = 0;
    let mut next = || {
        id += 1;
        id
    };
    let mut out = Vec::new();
    for b in bars {
        out.push(Marker {
            id: next(),
            time: b.start,
            end: None,
            label: format!("BAR {}", b.index + 1),
            color: theme::dim(),
            line: Line::Dotted,
        });
    }
    for e in &m.expected {
        out.push(Marker {
            id: next(),
            time: e.time,
            end: None,
            label: String::new(),
            color: theme::dim(),
            line: Line::Dotted,
        });
    }
    for p in &m.played {
        let grade = p
            .matched
            .and_then(|i| m.outcomes.get(i))
            .and_then(|o| o.grade);
        out.push(Marker {
            id: next(),
            time: p.onset,
            end: Some(p.offset.unwrap_or(p.onset)),
            label: String::new(),
            color: grade.map_or(theme::red(), theme::grade_color),
            line: Line::Solid,
        });
    }
    out
}

pub struct SessionReview {
    input: ReviewInput,
    score: Vec<ScoreBar>,
    overlays: Vec<Marker>,
    layout: StaffLayout,
    /// First and last selected bar, numbered from 1.
    pub from_bar: i64,
    pub to_bar: i64,
    click: bool,
    chords: bool,
    log: bool,
    replay: Option<ReplayHandle>,
    /// What to play once the running replay has stopped.
    queued: Option<(f64, f64)>,
    /// Annotations, in transport time.
    pub markers: Vec<Marker>,
    next_marker: u64,
    editor: Option<EditorState>,
    report_title: String,
    report_notes: String,
    export: ExportState,
}

impl SessionReview {
    pub fn new(input: ReviewInput) -> Self {
        let score = score::build(&ScoreInput {
            bars: &input.bars,
            matcher: &input.matcher,
            chords: &input.chords,
            scale: input.scale,
            a4: input.a4,
            until: f64::INFINITY,
        });
        let (lo, hi) = score::extent(&score);
        let overlays = overlays(&input.matcher, &score);
        let (from_bar, to_bar) = match (score.first(), score.last()) {
            (Some(a), Some(b)) => (a.index + 1, b.index + 1),
            _ => (1, 1),
        };
        let mut review = Self {
            score,
            overlays,
            layout: StaffLayout {
                spacing: STAFF_SPACING,
                lo,
                hi,
                labels: true,
            },
            from_bar,
            to_bar,
            click: true,
            chords: true,
            log: false,
            replay: None,
            queued: None,
            markers: Vec::new(),
            next_marker: 1 << 32,
            editor: None,
            report_title: String::new(),
            report_notes: String::new(),
            export: ExportState::Idle,
            input,
        };
        review.debug_open_editor();
        review
    }

    /// Debugging aid: `BASS_GYM_FAKE_SESSION=editor` opens the marker editor with a marker.
    fn debug_open_editor(&mut self) {
        if std::env::var("BASS_GYM_FAKE_SESSION").as_deref() != Ok("editor") {
            return;
        }
        let Some(note) = self.input.matcher.played.get(4) else {
            return;
        };
        let t = note.onset + 0.2;
        self.open_editor(t);
        if let Some(editor) = self.editor.as_mut() {
            editor.selected = Some(self.next_marker);
        }
        self.markers.push(Marker {
            id: self.next_marker,
            time: t,
            end: None,
            label: "note end should be detected here, and this is a long note to show \
                    how the label grows with what is typed into it"
                .into(),
            color: theme::cyan(),
            line: Line::Solid,
        });
    }

    pub fn recording(&self) -> &Arc<Recording> {
        &self.input.recording
    }

    /// Transport time of the selected bars.
    pub fn selected_range(&self) -> Option<(f64, f64)> {
        let selected = |b: &&ScoreBar| (self.from_bar - 1..self.to_bar).contains(&b.index);
        let first = self.score.iter().find(selected)?;
        let last = self.score.iter().rev().find(selected)?;
        Some((first.start, last.end))
    }

    fn is_playing(&self) -> bool {
        self.replay.is_some()
    }

    fn start_replay(&mut self, engine: &mut AudioEngine, from: f64, to: f64) {
        let handle = ReplayHandle::new(self.click, self.chords);
        let rec = self.input.recording.clone();
        if engine.start_replay(&rec, from, to, BASS_GAIN, handle.clone()) {
            self.replay = Some(handle);
        }
    }

    fn toggle_play(&mut self, engine: &mut AudioEngine) {
        self.queued = None;
        if self.is_playing() {
            engine.stop_session();
        } else if let Some((from, to)) = self.selected_range() {
            self.start_replay(engine, from, to);
        }
    }

    /// Plays from `from` to the end of the session, taking over from a running replay.
    fn play_from(&mut self, engine: &mut AudioEngine, from: f64) {
        let Some(to) = self.score.last().map(|b| b.end) else {
            return;
        };
        if self.is_playing() {
            // It takes the worker a moment to stop: `tick` starts this one after that.
            self.queued = Some((from, to));
            engine.stop_session();
        } else {
            self.start_replay(engine, from, to);
        }
    }

    /// Call on every frame: ends a replay when the dialog closed or playback finished.
    pub fn tick(&mut self, engine: &mut AudioEngine, dialog_open: bool) {
        if self.replay.is_none() {
            return;
        }
        if !dialog_open {
            self.queued = None;
            if engine.session_running() {
                engine.stop_session();
            }
        }
        if !engine.session_running() {
            engine.release_replay();
            self.replay = None;
            if let Some((from, to)) = self.queued.take() {
                self.start_replay(engine, from, to);
            }
        }
    }

    fn playhead(&self, engine: &AudioEngine) -> Option<f64> {
        self.replay.as_ref()?.position(engine.clock().now())
    }

    /// What a report of the selected bars is made from.
    fn report_input(&self) -> Option<ReportInput> {
        let (from, to) = self.selected_range()?;
        let i = &self.input;
        Some(ReportInput {
            title: self.report_title.clone(),
            notes: self.report_notes.clone(),
            exercise: i.exercise.clone(),
            settings: i.settings.clone(),
            a4: i.a4,
            recording: i.recording.clone(),
            bars: i.bars.clone(),
            matcher: i.matcher.clone(),
            chords: i.chords.clone(),
            annotations: self
                .markers
                .iter()
                .map(|m| Annotation {
                    time: m.time,
                    text: m.label.clone(),
                })
                .collect(),
            first_bar: self.from_bar,
            last_bar: self.to_bar,
            from,
            to,
        })
    }

    /// Writes the report on a thread of its own: a long session can take a while to zip.
    fn start_export(&mut self, data_dir: &Path) {
        let Some(input) = self.report_input() else {
            return;
        };
        let reports = data_dir.join(bg_report::REPORTS_FOLDER);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = bg_report::export(&input, &reports).map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
        });
        self.export = ExportState::Running(rx);
    }

    fn poll_export(&mut self) {
        if let ExportState::Running(rx) = &self.export {
            match rx.try_recv() {
                Ok(Ok(out)) => {
                    self.export = ExportState::Done {
                        folder: out.folder,
                        copied: false,
                    }
                }
                Ok(Err(e)) => self.export = ExportState::Failed(e),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.export = ExportState::Failed("the export stopped unexpectedly".into());
                }
            }
        }
    }

    fn report_row(&mut self, ui: &mut egui::Ui, data_dir: Option<&Path>) {
        self.poll_export();
        let running = matches!(self.export, ExportState::Running(_));
        ui.horizontal(|ui| {
            let export_width = 190.0;
            ui.add(
                text_field(&mut self.report_title)
                    .width(220.0)
                    .hint("Report title"),
            );
            let notes = ui.available_width() - export_width - 12.0;
            ui.add(
                text_field(&mut self.report_notes)
                    .width(notes.max(120.0))
                    .hint("What went wrong?"),
            );
            let export = ui.add_enabled(
                data_dir.is_some() && !running,
                button("Export selected bars").min_size([export_width, 30.0]),
            );
            if export.clicked()
                && let Some(dir) = data_dir
            {
                self.start_export(dir);
            }
        });
        match &mut self.export {
            ExportState::Idle => {}
            ExportState::Running(_) => {
                ui.ctx().request_repaint();
                ui.label(RichText::new("Exporting...").small().color(theme::dim()));
            }
            ExportState::Failed(e) => {
                ui.label(
                    RichText::new(format!("Export failed: {e}"))
                        .small()
                        .color(theme::red()),
                );
            }
            ExportState::Done { folder, copied } => {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Saved to {} (and as .zip)", folder.display()))
                            .small()
                            .color(theme::green()),
                    );
                    let label = if *copied {
                        format!("{} Copied", icons::CHECK)
                    } else {
                        format!("{} Copy path", icons::COPY)
                    };
                    if ui.add(button(label).small()).clicked() {
                        ui.ctx().copy_text(folder.display().to_string());
                        *copied = true;
                    }
                    if ui
                        .add(button(format!("{} Open folder", icons::FOLDER_OPEN)).small())
                        .clicked()
                    {
                        open_folder(folder);
                    }
                });
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, engine: &mut AudioEngine, data_dir: Option<&Path>) {
        if !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space))
        {
            self.toggle_play(engine);
        }
        if self.is_playing() {
            ui.ctx().request_repaint();
        }
        self.controls(ui, engine);
        ui.add_space(6.0);
        self.report_row(ui, data_dir);
        ui.add_space(10.0);
        let playhead = self.playhead(engine);
        // Only this part scrolls: the controls above stay in view.
        let height = ui.available_height();
        egui::ScrollArea::vertical()
            .id_salt("session_review")
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if self.editor.is_some() {
                    self.marker_editor(ui, playhead);
                } else {
                    self.rows(ui, engine, playhead);
                }
            });
    }

    fn controls(&mut self, ui: &mut egui::Ui, engine: &mut AudioEngine) {
        ui.horizontal(|ui| {
            let (icon, label) = if self.is_playing() {
                (icons::STOP, "Stop")
            } else {
                (icons::PLAY, "Play")
            };
            let can_play = self.is_playing() || !engine.session_running();
            if ui
                .add_enabled(
                    can_play,
                    button(format!("{icon} {label}")).min_size([84.0, 30.0]),
                )
                .on_hover_text(
                    "Plays the selected bars (Space). Click the score to play from there.",
                )
                .clicked()
            {
                self.toggle_play(engine);
            }
            if checkbox(ui, &mut self.click, "Click").changed()
                && let Some(h) = &self.replay
            {
                h.set_click(self.click);
            }
            if checkbox(ui, &mut self.chords, "Chords").changed()
                && let Some(h) = &self.replay
            {
                h.set_chords(self.chords);
            }
            ui.add_space(12.0);
            let at = self
                .playhead(engine)
                .or(self.selected_range().map(|r| r.0))
                .and_then(|t| position_at(&self.score, t));
            if let Some(at) = at {
                for (value, unit) in [
                    (at.bar, "BAR"),
                    (at.beat as i64, "BEAT"),
                    (at.sub as i64, "1/32"),
                ] {
                    ui.label(
                        RichText::new(value.to_string())
                            .font(mono(16.0))
                            .color(theme::text()),
                    );
                    ui.label(RichText::new(unit).font(mono(10.0)).color(theme::cyan()));
                }
            }
            ui.add_space(12.0);
            checkbox(ui, &mut self.log, "Log scale")
                .on_hover_text("Shows levels in dB, so quiet parts are easier to read");
            let (first, last) = match (self.score.first(), self.score.last()) {
                (Some(a), Some(b)) => (a.index + 1, b.index + 1),
                _ => (1, 1),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    number_field(&mut self.to_bar)
                        .range(first..=last)
                        .width(44.0)
                        .id_salt("to_bar"),
                );
                ui.label("to");
                ui.add(
                    number_field(&mut self.from_bar)
                        .range(first..=last)
                        .width(44.0)
                        .id_salt("from_bar"),
                );
                ui.label("Selected bars");
            });
            self.to_bar = self.to_bar.max(self.from_bar);
        });
    }

    fn rows(&mut self, ui: &mut egui::Ui, engine: &mut AudioEngine, playhead: Option<f64>) {
        if self.score.is_empty() {
            ui.label(RichText::new("No bars were played.").color(theme::dim()));
            return;
        }
        let width = ui.available_width();
        let lead = lead_width(STAFF_SPACING, true);
        let per_row = score::bars_per_row(&self.score, width - lead);
        let score_h = self.layout.height();
        let row_h = score_h + NOTE_LANE + WAVE_HEIGHT;
        let mut open_at = None;
        let mut select = None;
        let mut play = None;
        for (n, range) in score::rows(self.score.len(), per_row)
            .into_iter()
            .enumerate()
        {
            let (rect, resp) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
            ui.add_space(ROW_GAP);
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let bars = &self.score[range];
            let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
                continue;
            };
            // A short last row keeps the scale of the full ones.
            let bar_len = (last.end - first.start) / bars.len() as f64;
            let row_end = first.start + bar_len * per_row as f64;
            let left = rect.left() + lead;
            let axis = TimeAxis::new(Rangef::new(left, rect.right()), first.start, row_end);
            let right = axis.x(last.end);
            let staff =
                Rect::from_x_y_ranges(rect.left()..=right, rect.top()..=rect.top() + score_h);
            let wave =
                Rect::from_x_y_ranges(left..=right, (staff.bottom() + NOTE_LANE)..=rect.bottom());
            let p = ui.painter_at(rect.expand2(vec2(0.0, 2.0)));
            self.paint_selection(&p, bars, axis, rect);
            ScoreView {
                bars,
                axis,
                layout: self.layout,
                lead: Some(Lead {
                    clef: n == 0,
                    sig: first.sig,
                }),
            }
            .paint(&p, staff);
            self.paint_wave(&p, wave, axis, first.start, last.end, rect);
            if let Some(t) = playhead
                && (first.start..last.end).contains(&t)
            {
                p.vline(
                    axis.x(t),
                    rect.y_range(),
                    Stroke::new(2.0, theme::magenta()),
                );
            }
            if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked()) {
                if pos.y >= wave.top() {
                    open_at = Some(axis.t(pos.x));
                } else {
                    let t = axis.t(pos.x);
                    let bar = bars.iter().find(|b| b.start <= t && t < b.end);
                    let extend = ui.input(|i| i.modifiers.shift);
                    select = bar.map(|b| (b.index + 1, extend));
                    if !extend {
                        play = bar.map(|b| click_start(b, &self.input.matcher, axis, pos.x));
                    }
                }
            }
        }
        if let Some((bar, extend)) = select {
            if extend {
                self.from_bar = self.from_bar.min(bar);
                self.to_bar = self.to_bar.max(bar);
            } else {
                self.from_bar = bar;
                self.to_bar = bar;
            }
        }
        if let Some(t) = play {
            self.play_from(engine, t);
        }
        if let Some(t) = open_at {
            self.open_editor(t);
        }
    }

    fn paint_selection(&self, p: &egui::Painter, bars: &[ScoreBar], axis: TimeAxis, rect: Rect) {
        for b in bars {
            if (self.from_bar - 1..self.to_bar).contains(&b.index) {
                let r = Rect::from_x_y_ranges(axis.x(b.start)..=axis.x(b.end), rect.y_range());
                p.rect_filled(r, 0.0, theme::cyan().gamma_multiply(0.1));
            }
        }
    }

    fn paint_wave(
        &self,
        p: &egui::Painter,
        wave: Rect,
        axis: TimeAxis,
        from: f64,
        to: f64,
        row: Rect,
    ) {
        p.rect_filled(wave, 4.0, theme::bg());
        p.rect_stroke(
            wave,
            4.0,
            Stroke::new(1.0, theme::grid()),
            StrokeKind::Inside,
        );
        let rec = &self.input.recording;
        if rec.has_audio() {
            let style = WaveStyle {
                log: self.log,
                color: theme::text().gamma_multiply(0.65),
            };
            waveform::paint(p, wave, axis, &RecordingPeaks(rec), style);
            self.paint_missing_audio(p, wave, axis, from, to);
        } else {
            note(p, wave, "NO AUDIO RECORDED", theme::dim());
        }
        for m in &self.overlays {
            let end = m.end.unwrap_or(m.time);
            if end >= from && m.time < to && m.label.is_empty() {
                paint_marker(p, axis, m, row, wave);
            }
        }
        for m in self.markers.iter().filter(|m| (from..to).contains(&m.time)) {
            paint_marker(p, axis, m, row, wave);
        }
    }

    /// Marks audio that was overwritten or lost.
    fn paint_missing_audio(
        &self,
        p: &egui::Painter,
        wave: Rect,
        axis: TimeAxis,
        from: f64,
        to: f64,
    ) {
        let rec = &self.input.recording;
        let gone = rec.first_time().min(to);
        if gone > from {
            let r = Rect::from_x_y_ranges(axis.x(from)..=axis.x(gone), wave.y_range());
            p.rect_filled(r, 0.0, theme::raised());
            note(p, r, "OVERWRITTEN", theme::dim());
        }
        for g in rec
            .gaps
            .iter()
            .filter(|g| g.time < to && g.time + g.seconds > from)
        {
            let x = axis.x(g.time);
            let r =
                Rect::from_x_y_ranges(x..=axis.x(g.time + g.seconds).max(x + 2.0), wave.y_range());
            p.rect_filled(r, 0.0, theme::amber().gamma_multiply(0.4));
        }
    }

    fn open_editor(&mut self, t: f64) {
        let rec = &self.input.recording;
        if !rec.has_audio() {
            return;
        }
        let bounds = (rec.first_time(), rec.end_time());
        let view = WaveView::new(t - EDITOR_SPAN, t + EDITOR_SPAN, bounds);
        let mut state = EditorState::new(view);
        state.log = self.log;
        self.editor = Some(state);
    }

    fn marker_editor(&mut self, ui: &mut egui::Ui, playhead: Option<f64>) {
        ui.horizontal(|ui| {
            if ui
                .add(button(format!("{} Back to score", icons::ARROW_LEFT)).small())
                .clicked()
            {
                self.editor = None;
            }
            ui.label(
                RichText::new(
                    "Drag down to zoom in, up to zoom out, sideways to move. Double-click to add a marker.",
                )
                .small()
                .color(theme::dim()),
            );
        });
        let Some(state) = self.editor.as_mut() else {
            return;
        };
        ui.add_space(6.0);
        let next = &mut self.next_marker;
        let mut new_marker = |time| {
            *next += 1;
            Marker {
                id: *next,
                time,
                end: None,
                label: String::new(),
                color: theme::cyan(),
                line: Line::Solid,
            }
        };
        let score = &self.score;
        // Bars are named by the editor's own markers.
        let layout = StaffLayout {
            labels: false,
            ..self.layout
        };
        let paint_score = |p: &egui::Painter, rect: Rect, axis: TimeAxis| {
            let (from, to) = (axis.t(rect.left()), axis.t(rect.right()));
            // With the bar before, for notes tied into the view.
            let first = score.partition_point(|b| b.end < from).saturating_sub(1);
            let last = score.partition_point(|b| b.start <= to).max(first);
            ScoreView {
                bars: &score[first..last],
                axis,
                layout,
                lead: None,
            }
            .paint(p, rect);
        };
        MarkerEditor {
            header: Some(Header {
                height: layout.height(),
                paint: &paint_score,
            }),
            source: &RecordingPeaks(&self.input.recording),
            overlays: &self.overlays,
            markers: &mut self.markers,
            state,
            snap: &|t| t,
            add_remove: true,
            new_marker: &mut new_marker,
            playhead,
        }
        .show(ui, EDITOR_HEIGHT);
    }
}

fn note(p: &egui::Painter, rect: Rect, text: &str, color: Color32) {
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        mono(11.0),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_core::TimeSignature;

    fn bar(index: i64, start: f64) -> ScoreBar {
        ScoreBar {
            index,
            start,
            end: start + 2.0,
            bpm: 120.0,
            show_bpm: false,
            sig: TimeSignature::default(),
            events: Vec::new(),
        }
    }

    #[test]
    fn positions_count_bars_beats_and_32nds_from_one() {
        let bars = [bar(0, 10.0), bar(1, 12.0)];
        let at = |t| position_at(&bars, t).unwrap();
        assert_eq!(
            at(10.0),
            MusicPosition {
                bar: 1,
                beat: 1,
                sub: 1
            }
        );
        // 13.0 is beat 3 of bar 2; a 32nd of a 0.5 s beat is 62.5 ms.
        assert_eq!(
            at(13.0),
            MusicPosition {
                bar: 2,
                beat: 3,
                sub: 1
            }
        );
        assert_eq!(
            at(13.07),
            MusicPosition {
                bar: 2,
                beat: 3,
                sub: 2
            }
        );
        assert_eq!(
            at(13.99),
            MusicPosition {
                bar: 2,
                beat: 4,
                sub: 8
            }
        );
    }

    #[test]
    fn position_before_the_first_bar_is_its_start() {
        let bars = [bar(0, 10.0)];
        let p = position_at(&bars, 5.0).unwrap();
        assert_eq!((p.bar, p.beat, p.sub), (1, 1, 1));
        assert_eq!(position_at(&[], 5.0), None);
    }

    #[test]
    fn a_click_plays_from_the_note_there_or_the_start_of_the_bar() {
        use bg_core::score::{NoteValue, ScoreNote};
        let mut m = Matcher::default();
        for onset in [10.52, 11.03] {
            m.played.push(bg_core::scoring::Played {
                onset,
                offset: None,
                peak_db: -10.0,
                freq: None,
                harmony: None,
                matched: None,
            });
        }
        let event = |time, kind| ScoreEvent {
            time,
            beat: 0,
            value: NoteValue {
                denominator: 4,
                dots: 0,
            },
            tuplet: None,
            kind,
        };
        let note = |played| {
            ScoreKind::Note(ScoreNote {
                played,
                pitch: None,
                clash: false,
                chord_marks: Vec::new(),
                tied: false,
                continuation: false,
                stem_up: true,
                beam_group: None,
            })
        };
        let mut b = bar(0, 10.0);
        b.events = vec![
            event(10.5, note(0)),
            event(11.0, note(1)),
            event(11.5, ScoreKind::Rest),
        ];
        // 100 points per second, starting at the bar.
        let axis = TimeAxis::new(Rangef::new(0.0, 200.0), 10.0, 12.0);
        let start = |x| click_start(&b, &m, axis, x);
        // Notes are written on the grid, but play from where they really were.
        assert!((start(52.0) - (10.52 - NOTE_LEAD)).abs() < 1e-9);
        assert!((start(95.0) - (11.03 - NOTE_LEAD)).abs() < 1e-9);
        assert_eq!(start(20.0), 10.0);
        // A rest isn't a place of its own.
        assert_eq!(start(150.0), 10.0);
    }

    #[test]
    fn overlays_use_grade_colours_and_red_for_extras() {
        let mut m = Matcher::default();
        m.played.push(bg_core::scoring::Played {
            onset: 1.0,
            offset: Some(1.5),
            peak_db: -10.0,
            freq: None,
            harmony: None,
            matched: None,
        });
        let o = overlays(&m, &[bar(0, 0.0)]);
        assert_eq!(o.len(), 2);
        assert_eq!(o[1].color, theme::red());
        assert_eq!(o[1].end, Some(1.5));
    }
}
