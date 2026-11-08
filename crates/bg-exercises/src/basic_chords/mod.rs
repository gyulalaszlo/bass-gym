//! Basic chords: a simulated keyboard player comps a progression in the chosen key and
//! the bassist plays chord-compatible notes on the groove. A piano keyboard shows what
//! the player's hands are doing.

pub mod accompaniment;
pub mod prefs;

use crate::fake_session;
use anyhow::Result;
use std::sync::Arc;

use bg_core::harmony::{self, Chord, Mode, Scale, comp};
use bg_core::music::note_name;
use bg_core::preset::{PresetError, PresetSchema};
use bg_core::score::ScoreChord;
use bg_core::scoring::harmony_grade;
use bg_core::{Grade, SessionConfig};
use bg_ui::recording::{RecordingPrefs, SessionCapture};
use bg_ui::view::ViewPrefs;
use bg_ui::widgets::beatgrid::{Beatgrid, LiveScore};
use bg_ui::widgets::button::{button, selectable_value};
use bg_ui::widgets::controls::{checkbox, dropdown, number_field, text_field};
use bg_ui::widgets::keyboard::{self, KeyLabels, Keyboard};
use bg_ui::widgets::performance::performance_strip;
use bg_ui::widgets::results::{ResultsTab, block, results, results_dialog};
use bg_ui::widgets::session_panel::{session_panel, session_toolbar};
use bg_ui::widgets::session_review::{ReviewInput, SessionReview, report_settings};
use bg_ui::widgets::toolbar::{Gap, Toolbar};
use bg_ui::widgets::tuner::TunerPrefs;
use bg_ui::{ExerciseCtx, ExerciseType, PreferencePage, egui, icons, theme};
use egui::RichText;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use self::accompaniment::{ChordAccompaniment, SharedChords, lock_chords, preview_targets};
use self::prefs::{BasicChordsPage, BasicChordsPrefs};
use crate::timing_session::{
    AccompanimentScoring, PERFORMANCE_HEIGHT, SharedTiming, TimingSession, judge_time, last_time,
    lock, timing_meter,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressionKind {
    /// A chosen library progression or a typed one.
    #[default]
    Fixed,
    /// A random pick from (a selection of) the library on every start.
    SemiRandom,
    /// A generated in-key progression.
    Random,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixedSource {
    #[default]
    Library,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressionSettings {
    pub kind: ProgressionKind,
    pub source: FixedSource,
    /// Library id for `Fixed` + `Library`.
    pub library: String,
    /// Roman numerals for `Fixed` + `Custom`.
    pub custom: String,
    /// Library ids to pick from in `SemiRandom`; empty = all.
    pub pool: Vec<String>,
    pub random_len: u32,
}

impl Default for ProgressionSettings {
    fn default() -> Self {
        Self {
            kind: ProgressionKind::Fixed,
            source: FixedSource::Library,
            library: "pop".into(),
            custom: "ii7 V7 I7 vi7".into(),
            pool: Vec::new(),
            random_len: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChordLength {
    Half,
    #[default]
    One,
    Two,
    Four,
}

impl ChordLength {
    pub const ALL: [ChordLength; 4] = [Self::Half, Self::One, Self::Two, Self::Four];

    pub fn bars(self) -> f32 {
        match self {
            Self::Half => 0.5,
            Self::One => 1.0,
            Self::Two => 2.0,
            Self::Four => 4.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Half => "½ bar",
            Self::One => "1 bar",
            Self::Two => "2 bars",
            Self::Four => "4 bars",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardOptions {
    pub keys: u32,
    pub labels: KeyLabels,
    /// Outline the next chord shortly before it starts.
    pub ghost_next: bool,
    /// Hide chord names, to practise reading the keyboard.
    pub hide_chord_names: bool,
    pub show_bass: bool,
}

impl Default for KeyboardOptions {
    fn default() -> Self {
        Self {
            keys: 49,
            labels: KeyLabels::Octaves,
            ghost_next: true,
            hide_chord_names: false,
            show_bass: true,
        }
    }
}

/// What a session grades: which rhythms are targets, and whether notes must fit the chord.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoringOptions {
    /// The groove's notes are targets.
    pub groove: bool,
    /// The piano's attacks are targets.
    pub piano: bool,
    /// Played notes are graded by how they fit the chord (see `checker`).
    pub chord_tones: bool,
}

impl Default for ScoringOptions {
    fn default() -> Self {
        Self {
            groove: true,
            piano: false,
            chord_tones: true,
        }
    }
}

impl ScoringOptions {
    pub fn has_targets(&self) -> bool {
        self.groove || self.piano
    }

    fn summary(&self) -> &'static str {
        match (self.has_targets(), self.chord_tones) {
            (false, false) => "Free play: nothing is graded.",
            (false, true) => "Any rhythm: only how the notes fit the chords is graded.",
            (true, false) => "Rhythm only: any pitch is fine.",
            (true, true) => "Rhythm and chord fit are both graded.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BasicChordsData {
    pub session: SessionConfig,
    pub scale: Scale,
    pub progression: ProgressionSettings,
    pub chord_length: ChordLength,
    /// Comp pattern id (see `harmony::comp::PATTERNS`).
    pub comp: String,
    /// 0 = plain triads … 1 = rich, rootless voicings.
    pub jazziness: f32,
    /// Voicing width: 0 = one hand, 1 = two hands, 2 = two hands with octave doublings.
    pub width: f32,
    /// Register of the comp, in octaves (4 = around middle C).
    pub center_octave: i8,
    /// 0 = machine-exact … 1 = loose timing and dynamics.
    pub humanize: f32,
    pub piano_volume: f32,
    /// Chord-compatibility checker id.
    pub checker: String,
    pub scoring: ScoringOptions,
    pub seed: u64,
    pub new_seed_each_start: bool,
    pub keyboard: KeyboardOptions,
}

impl Default for BasicChordsData {
    fn default() -> Self {
        Self {
            session: SessionConfig::default(),
            scale: Scale::default(),
            progression: ProgressionSettings::default(),
            chord_length: ChordLength::One,
            comp: "halves".into(),
            jazziness: 0.2,
            width: 0.0,
            center_octave: harmony::voicing::CENTER_OCTAVE,
            humanize: 0.0,
            piano_volume: 0.7,
            checker: "chord_tones".into(),
            scoring: ScoringOptions::default(),
            seed: 1,
            new_seed_each_start: false,
            keyboard: KeyboardOptions::default(),
        }
    }
}

/// The chords a session with `d` and `seed` will play.
pub fn resolve_progression(d: &BasicChordsData, seed: u64) -> Result<Vec<Chord>, String> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let p = &d.progression;
    let text = match p.kind {
        ProgressionKind::Random => {
            return Ok(harmony::random_progression(
                &d.scale,
                p.random_len.max(1) as usize,
                d.jazziness,
                &mut rng,
            ));
        }
        ProgressionKind::Fixed => match p.source {
            FixedSource::Library => harmony::library(&p.library).map_or("I", |e| e.2).to_owned(),
            FixedSource::Custom => p.custom.clone(),
        },
        ProgressionKind::SemiRandom => {
            use rand::Rng;
            let pool: Vec<&str> = harmony::LIBRARY
                .iter()
                .filter(|e| p.pool.is_empty() || p.pool.iter().any(|id| id == e.0))
                .map(|e| e.2)
                .collect();
            let pool = if pool.is_empty() {
                vec![harmony::LIBRARY[0].2]
            } else {
                pool
            };
            pool[rng.random_range(0..pool.len())].to_owned()
        }
    };
    let specs = harmony::parse_progression(&text, &d.scale).map_err(|e| e.to_string())?;
    Ok(harmony::resolve(&specs, &d.scale, d.jazziness, &mut rng))
}

#[derive(Default)]
pub struct BasicChords {
    data: BasicChordsData,
    timing: SharedTiming,
    chords: SharedChords,
    has_results: bool,
    /// The results dialog is open.
    show_results: bool,
    /// Running on the previous frame (to open the results when the session ends).
    was_running: bool,
    /// Seed of the last started session.
    run_seed: Option<u64>,
    error: Option<String>,
    /// Tuning reference, for naming played pitches.
    a4: f32,
    capture: SessionCapture,
    review: Option<SessionReview>,
    results_tab: ResultsTab,
}

impl PresetSchema for BasicChords {
    fn current_version(&self) -> u32 {
        2
    }

    fn migrate_step(&self, from: u32, mut data: Value) -> Result<Value, PresetError> {
        match from {
            // v1 ("Chord interaction" preview): sevenths flag, random progression length
            // and whole bars per chord.
            1 => {
                let o = data
                    .as_object_mut()
                    .ok_or_else(|| PresetError::Invalid("expected an object".into()))?;
                let sevenths = o
                    .remove("sevenths")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let len = o
                    .remove("progression_len")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(4);
                let bars = o
                    .remove("bars_per_chord")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1);
                o.insert("jazziness".into(), json!(if sevenths { 0.35 } else { 0.1 }));
                o.insert(
                    "progression".into(),
                    json!({ "kind": "Random", "random_len": len }),
                );
                o.insert(
                    "chord_length".into(),
                    json!(match bars {
                        0 | 1 => "One",
                        2 => "Two",
                        _ => "Four",
                    }),
                );
                Ok(data)
            }
            _ => Err(PresetError::NoMigration(from)),
        }
    }
}

const JAZZ_LEVELS: [&str; 4] = [
    "Triads in close position, root in the left hand",
    "Seventh chords with smooth inversions",
    "Ninths, open (drop-2) voicings, left-hand shells",
    "Rich colour (9/11/13), rootless voicings, anticipations",
];

fn jazz_description(j: f32) -> &'static str {
    JAZZ_LEVELS[((j * 4.0) as usize).min(3)]
}

impl ExerciseType for BasicChords {
    fn id(&self) -> &'static str {
        // Kept from the "Chord interaction" preview so its presets still apply.
        "diatonic_chords"
    }

    fn name(&self) -> &'static str {
        "Basic chords"
    }

    fn icon(&self) -> &'static str {
        icons::PIANO_KEYS
    }

    fn description(&self) -> &'static str {
        "A keyboard player comps a progression; play chord-compatible notes on the groove."
    }

    fn default_preset(&self) -> Value {
        serde_json::to_value(BasicChordsData::default()).unwrap_or_default()
    }

    fn save_preset(&self) -> Value {
        serde_json::to_value(&self.data).unwrap_or_default()
    }

    fn load_preset(&mut self, data: Value) -> Result<()> {
        self.data = serde_json::from_value(data)?;
        self.has_results = false;
        self.run_seed = None;
        Ok(())
    }

    fn is_running(&self) -> bool {
        lock(&self.timing).running
    }

    fn preference_pages(&self) -> Vec<Box<dyn PreferencePage>> {
        vec![Box::new(BasicChordsPage)]
    }

    fn toolbar(&mut self, tb: &mut Toolbar, ctx: &mut ExerciseCtx) {
        self.a4 = ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4;
        let running = ctx.engine.session_running() && self.is_running();
        if self.was_running && !running && self.has_results {
            self.show_results = true;
        }
        self.was_running = running;
        self.capture.poll(ctx.engine);
        if !self.has_results {
            self.capture.clear();
            self.review = None;
        }
        self.update_review(ctx);
        if tb.transport(running) {
            if running {
                ctx.engine.stop_session();
            } else if ctx.engine.clock().is_running() {
                self.start(ctx);
            }
        }
        tb.gap(Gap::Small);
        if tb.results_button(self.has_results && !running) {
            self.show_results = true;
        }
        // Current / next chord in the space between START and the settings.
        let now = if running {
            ctx.engine.clock().now()
        } else {
            last_time(&self.timing)
        };
        tb.gap_with(Gap::Large, |ui| self.status(ui, now, running));
        if tb.group(!running, |tb| session_toolbar(tb, &mut self.data.session)) {
            self.has_results = false;
            self.run_seed = None;
        }
    }

    fn notices(&mut self, ui: &mut egui::Ui, ctx: &mut ExerciseCtx) {
        if let Some(e) = &self.error {
            ui.label(RichText::new(format!("{} {e}", icons::WARNING)).color(theme::red()));
        }
        if !ctx.engine.clock().is_running() {
            ui.label(
                RichText::new(format!(
                    "{} No audio output — check Preferences › Audio",
                    icons::WARNING
                ))
                .color(theme::amber()),
            );
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut ExerciseCtx) {
        let running = ctx.engine.session_running() && self.is_running();
        self.a4 = ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4;
        let now = if running {
            ctx.engine.clock().now()
        } else {
            last_time(&self.timing)
        };

        ui.add_space(6.0);
        self.keyboard(ui, now);
        ui.add_space(6.0);
        self.progression_strip(ui, now);
        ui.add_space(6.0);
        let prefs: ViewPrefs = ctx.settings.get(ViewPrefs::ID);
        let meter = timing_meter(
            &self.timing,
            self.has_results,
            now,
            prefs.indicator_bars,
            ctx.scoring,
        );
        performance_strip(ui, PERFORMANCE_HEIGHT, &[&meter]);
        ui.add_space(4.0);
        self.grid(
            ui,
            now,
            &prefs,
            ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4,
        );
        if running {
            ui.ctx().request_repaint();
        }
        ui.add_space(8.0);
        if self.config(ui, !running) {
            self.has_results = false;
            self.run_seed = None;
        }

        let mut open = self.show_results && self.has_results;
        let stats = open.then(|| lock(&self.timing).matcher.stats());
        // Out of `self` while the dialog runs: the tables below borrow all of it.
        let mut review = self.review.take();
        let mut results_tab = self.results_tab;
        let tab = review.is_some().then_some(&mut results_tab);
        let scoring = ctx.scoring;
        let engine = &mut *ctx.engine;
        let data_dir = ctx.data_dir;
        results_dialog(ui.ctx(), &mut open, "Results", tab, |ui, tab| {
            match (tab, review.as_mut(), &stats) {
                (ResultsTab::Review, Some(r), _) => r.ui(ui, engine, data_dir),
                (_, _, Some(stats)) => results(ui, stats, scoring, |ui| {
                    block(ui, "Per chord", |ui| self.per_chord_table(ui))
                }),
                _ => {}
            }
        });
        if let Some(r) = review.as_mut() {
            r.tick(ctx.engine, open);
        }
        self.review = review;
        self.results_tab = results_tab;
        self.show_results = open;
    }

    fn debug_fake_session(&mut self, ctx: &mut ExerciseCtx) {
        self.a4 = ctx.settings.get::<TunerPrefs>(TunerPrefs::ID).a4;
        self.data.session = fake_session::config(&self.data.session);
        let Some((logic, seed)) = self.session(ctx) else {
            return;
        };
        let mut logic = logic;
        let recording = fake_session::run(&mut logic, &self.timing, self.a4);
        self.capture.set(Arc::new(recording));
        self.has_results = true;
        self.show_results = fake_session::opens_results();
        self.results_tab = ResultsTab::Review;
        self.run_seed = Some(seed);
    }
}

impl BasicChords {
    /// Sets up the review once the session has fully ended.
    fn update_review(&mut self, ctx: &mut ExerciseCtx) {
        if self.review.is_some() || ctx.engine.session_running() {
            return;
        }
        let Some(recording) = self.capture.recording().cloned() else {
            return;
        };
        // One lock at a time: chords first, then timing.
        let chords: Vec<ScoreChord> = lock_chords(&self.chords)
            .spans
            .iter()
            .map(|s| ScoreChord {
                start: s.start,
                end: s.end,
                tones: s.chord.tones(),
                label: self.chord_label(&s.chord),
            })
            .collect();
        let settings = report_settings(ctx, self.save_preset());
        let shared = lock(&self.timing);
        self.review = Some(SessionReview::new(ReviewInput {
            bars: shared.bars.clone(),
            matcher: shared.matcher.clone(),
            chords,
            scale: Some(self.data.scale),
            a4: self.a4,
            recording,
            exercise: self.id().to_owned(),
            settings,
        }));
    }

    fn start(&mut self, ctx: &mut ExerciseCtx) {
        let Some((logic, seed)) = self.session(ctx) else {
            return;
        };
        let max_bytes = ctx
            .settings
            .get::<RecordingPrefs>(RecordingPrefs::ID)
            .max_bytes();
        let id = ctx.engine.start_recorded(Box::new(logic), max_bytes);
        self.capture.begin(id);
        self.review = None;
        self.results_tab = ResultsTab::Stats;
        self.has_results = true;
        self.show_results = false;
        self.run_seed = Some(seed);
    }

    /// Builds a session from the settings, with the seed it uses.
    fn session(&mut self, ctx: &mut ExerciseCtx) -> Option<(TimingSession, u64)> {
        let seed = if self.data.new_seed_each_start {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.as_nanos() as u64)
        } else {
            self.data.seed
        };
        let progression = match resolve_progression(&self.data, seed) {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(format!("Can't start: {e}"));
                return None;
            }
        };
        self.error = None;
        let d = &self.data;
        let a4 = self.a4;
        let mode = ctx
            .settings
            .get::<BasicChordsPrefs>(BasicChordsPrefs::ID)
            .grade_mode;
        let acc = ChordAccompaniment::new(
            progression,
            d.scale,
            harmony::checker(&d.checker),
            d.comp.clone(),
            d.chord_length.bars(),
            harmony::VoicingStyle {
                jazziness: d.jazziness,
                width: d.width,
                center_octave: d.center_octave,
            },
            d.piano_volume,
            a4,
            seed,
            self.chords.clone(),
        )
        .with_humanize(d.humanize);
        let logic = TimingSession::with_accompaniment(
            d.session.clone(),
            ctx.scoring.clone(),
            self.timing.clone(),
            Box::new(acc),
            AccompanimentScoring {
                grade_mode: mode,
                groove: d.scoring.groove,
                hits: d.scoring.piano,
                harmony: d.scoring.chord_tones,
            },
        );
        Some((logic, seed))
    }

    /// The progression to show: the running / last one, or a preview of the settings.
    fn shown_progression(&self) -> Vec<Chord> {
        if self.has_results {
            let p = lock_chords(&self.chords).progression.clone();
            if !p.is_empty() {
                return p;
            }
        }
        resolve_progression(&self.data, self.run_seed.unwrap_or(self.data.seed)).unwrap_or_default()
    }

    fn chord_label(&self, c: &Chord) -> String {
        if self.data.keyboard.hide_chord_names {
            "?".into()
        } else {
            c.name()
        }
    }

    fn status(&self, ui: &mut egui::Ui, now: f64, running: bool) {
        if !running {
            return;
        }
        let shared = lock_chords(&self.chords);
        let current = shared.spans.iter().find(|s| s.start <= now && now < s.end);
        let next = shared.spans.iter().find(|s| s.start > now);
        match current {
            Some(c) => {
                ui.label(
                    RichText::new(self.chord_label(&c.chord))
                        .font(theme::mono(26.0))
                        .color(theme::cyan()),
                );
                if !self.data.keyboard.hide_chord_names {
                    ui.label(
                        RichText::new(c.chord.roman())
                            .font(theme::mono(16.0))
                            .color(theme::dim()),
                    );
                }
            }
            None => {
                ui.label(
                    RichText::new("COUNT-IN")
                        .font(theme::mono(18.0))
                        .color(theme::amber()),
                );
            }
        }
        if let Some(n) = next {
            ui.label(
                RichText::new("NEXT")
                    .font(theme::mono(13.0))
                    .color(theme::text()),
            );
            ui.label(
                RichText::new(self.chord_label(&n.chord))
                    .font(theme::mono(16.0))
                    .color(theme::magenta()),
            );
        }
    }

    fn keyboard(&self, ui: &mut egui::Ui, now: f64) {
        let opts = &self.data.keyboard;
        let (active, ghost) = if self.has_results {
            let shared = lock_chords(&self.chords);
            let active: Vec<u8> = shared
                .piano
                .iter()
                .filter(|n| n.time <= now && now < n.time + n.len)
                .map(|n| n.midi)
                .collect();
            let ghost = if opts.ghost_next {
                let beat = lock(&self.timing)
                    .bars
                    .iter()
                    .rev()
                    .find(|b| b.start <= now)
                    .map_or(0.5, |b| b.duration / b.beats.max(1) as f64);
                shared
                    .spans
                    .iter()
                    .find(|s| s.start > now && s.start - now <= beat)
                    .map(|s| s.voicing.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            (active, ghost)
        } else {
            (Vec::new(), Vec::new())
        };
        let bass = if opts.show_bass && self.has_results {
            let t = lock(&self.timing);
            t.matcher.played.last().and_then(|p| {
                let freq = p.freq?;
                let recent = p.offset.is_none_or(|o| now - o < 1.5);
                recent.then(|| {
                    let midi = bg_core::music::freq_to_midi(freq, self.a4).0;
                    let color = p
                        .harmony
                        .map_or(theme::dim(), |h| theme::grade_color(harmony_grade(h)));
                    (midi, color)
                })
            })
        } else {
            None
        };
        Keyboard {
            keys: opts.keys,
            active: &active,
            ghost: &ghost,
            bass,
            labels: opts.labels,
        }
        .show(ui);
    }

    fn progression_strip(&self, ui: &mut egui::Ui, now: f64) {
        let progression = self.shown_progression();
        let current = if self.has_results {
            lock_chords(&self.chords)
                .spans
                .iter()
                .find(|s| s.start <= now && now < s.end)
                .map(|s| s.step)
        } else {
            None
        };
        // One row of compact cells (fixed height, scrolls sideways if it doesn't fit).
        let hide = self.data.keyboard.hide_chord_names;
        egui::ScrollArea::horizontal()
            .id_salt("bc_strip")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (i, c) in progression.iter().enumerate() {
                        let active = current == Some(i);
                        let name_color = if active { theme::text() } else { theme::cyan() };
                        let name = ui.painter().layout_no_wrap(
                            self.chord_label(c),
                            theme::mono(18.0),
                            name_color,
                        );
                        let roman = ui.painter().layout_no_wrap(
                            if hide { String::new() } else { c.roman() },
                            theme::mono(12.0),
                            theme::dim(),
                        );
                        let size = egui::vec2(name.size().x.max(roman.size().x) + 20.0, 48.0);
                        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        let p = ui.painter();
                        if active {
                            p.rect_filled(rect, 2.0, theme::cyan().gamma_multiply(0.18));
                            p.rect_stroke(
                                rect,
                                2.0,
                                egui::Stroke::new(1.5, theme::cyan()),
                                egui::StrokeKind::Inside,
                            );
                        } else {
                            p.rect_stroke(
                                rect,
                                2.0,
                                egui::Stroke::new(1.0, theme::grid()),
                                egui::StrokeKind::Inside,
                            );
                        }
                        p.galley(
                            egui::pos2(rect.center().x - name.size().x / 2.0, rect.top() + 5.0),
                            name,
                            name_color,
                        );
                        p.galley(
                            egui::pos2(
                                rect.center().x - roman.size().x / 2.0,
                                rect.bottom() - 5.0 - roman.size().y,
                            ),
                            roman,
                            theme::dim(),
                        );
                    }
                });
            });
    }

    fn grid(&self, ui: &mut egui::Ui, now: f64, prefs: &ViewPrefs, a4: f32) {
        let steps = self.data.session.groove.steps_per_beat;
        let (show_score, size) = (prefs.show_score, prefs.score_size);
        let zoom = prefs.zoom_factor();
        let live = |chords| {
            show_score.then_some(LiveScore {
                chords,
                scale: Some(self.data.scale),
                a4,
                size,
            })
        };
        if self.has_results {
            // One lock at a time: chords first, then timing.
            let (labels, chords): (Vec<(f64, f64, String)>, Vec<ScoreChord>) =
                lock_chords(&self.chords)
                    .spans
                    .iter()
                    .map(|s| {
                        let label = self.chord_label(&s.chord);
                        let chord = ScoreChord {
                            start: s.start,
                            end: s.end,
                            tones: s.chord.tones(),
                            label: label.clone(),
                        };
                        ((s.start, s.end, label), chord)
                    })
                    .unzip();
            let t = lock(&self.timing);
            Beatgrid {
                now,
                bars: &t.bars,
                matcher: &t.matcher,
                steps_per_beat: steps,
                bars_behind: 1.0,
                bars_ahead: 2.0,
                chords: &labels,
                score: live(&chords),
            }
            .show(ui, 180.0, zoom);
        } else {
            let d = &self.data;
            let bars_per_chord = d.chord_length.bars();
            let (bars, matcher) = chord_preview(&d.session, &d.scoring, &d.comp, bars_per_chord);
            let progression = self.shown_progression();
            let labels: Vec<(f64, f64, String)> = if progression.is_empty() {
                Vec::new()
            } else {
                let len =
                    bars.first().map_or(2.0, |b| b.duration) * self.data.chord_length.bars() as f64;
                (0..8)
                    .map(|i| {
                        (
                            i as f64 * len,
                            (i + 1) as f64 * len,
                            self.chord_label(&progression[i % progression.len()]),
                        )
                    })
                    .collect()
            };
            let now = bars.first().map_or(0.0, |b| b.start);
            Beatgrid {
                now,
                bars: &bars,
                matcher: &matcher,
                steps_per_beat: steps,
                bars_behind: 1.0,
                bars_ahead: 2.0,
                chords: &labels,
                score: live(&[]),
            }
            .show(ui, 180.0, zoom);
        }
    }

    fn per_chord_table(&self, ui: &mut egui::Ui) {
        let chords = lock_chords(&self.chords);
        let t = lock(&self.timing);
        let m = &t.matcher;
        if chords.progression.is_empty() || (m.expected.is_empty() && m.played.is_empty()) {
            return;
        }
        let step_at = |time: f64| {
            chords
                .spans
                .iter()
                .find(|s| s.start <= time && time < s.end)
                .map(|s| s.step)
        };
        // Per progression step: targets, timing hits, chord tones, judged pitches.
        let mut rows = vec![(0usize, 0usize, 0usize, 0usize); chords.progression.len()];
        for (e, o) in m.expected.iter().zip(&m.outcomes) {
            let (Some(g), Some(step)) = (o.grade, step_at(e.time)) else {
                continue;
            };
            rows[step].0 += 1;
            if g.is_hit() {
                rows[step].1 += 1;
            }
        }
        // Every judged note counts for harmony, extras included, under the chord it was
        // judged against.
        for p in &m.played {
            let Some(h) = p.harmony else { continue };
            if let Some(step) = step_at(judge_time(m, p)) {
                rows[step].3 += 1;
                if harmony_grade(h) == Grade::Good {
                    rows[step].2 += 1;
                }
            }
        }
        let pct = |a: usize, b: usize| {
            if b == 0 {
                "--".to_owned()
            } else {
                format!("{:.0}%", a as f32 * 100.0 / b as f32)
            }
        };
        let (targets, fit) = (
            self.data.scoring.has_targets(),
            self.data.scoring.chord_tones,
        );
        let cell = |ui: &mut egui::Ui, text: String| {
            ui.label(RichText::new(text).font(theme::mono(14.0)));
        };
        egui::Grid::new("per_chord")
            .spacing(egui::vec2(18.0, 2.0))
            .show(ui, |ui| {
                let heads = [
                    ("CHORD", true),
                    ("NOTES", targets),
                    ("HIT", targets),
                    ("IN CHORD", fit),
                ];
                for (h, _) in heads.iter().filter(|h| h.1) {
                    ui.label(RichText::new(*h).small().color(theme::dim()));
                }
                ui.end_row();
                for (c, r) in chords.progression.iter().zip(&rows) {
                    ui.label(
                        RichText::new(self.chord_label(c))
                            .font(theme::mono(14.0))
                            .color(theme::cyan()),
                    );
                    if targets {
                        cell(ui, r.0.to_string());
                        cell(ui, pct(r.1, r.0));
                    }
                    if fit {
                        cell(ui, pct(r.2, r.3));
                    }
                    ui.end_row();
                }
            });
    }

    /// The configuration panels. Returns true when anything changed.
    fn config(&mut self, ui: &mut egui::Ui, enabled: bool) -> bool {
        let before = self.data.clone();
        let run_seed = self.run_seed;
        let d = &mut self.data;
        ui.add_enabled_ui(enabled, |ui| {
            theme::responsive_columns(ui, 330.0, 4, |ui, i| match i {
                0 => theme::section(ui, "Key", |ui| scale_ui(ui, d)),
                1 => theme::section(ui, "Progression", |ui| progression_ui(ui, d, run_seed)),
                2 => theme::section(ui, "Piano", |ui| piano_ui(ui, d)),
                _ => theme::section(ui, "Scoring", |ui| scoring_ui(ui, d)),
            });
        });
        ui.add_space(6.0);
        session_panel(ui, &mut self.data.session, enabled);
        self.data != before
    }
}

/// The preview grid's bars and targets for these settings.
fn chord_preview(
    session: &SessionConfig,
    scoring: &ScoringOptions,
    comp: &str,
    bars_per_chord: f32,
) -> (Vec<bg_core::BarPlan>, bg_core::Matcher) {
    crate::timing_session::preview_targets(session, scoring.groove, |bar| {
        if scoring.piano {
            preview_targets(comp, bar, bars_per_chord)
        } else {
            Vec::new()
        }
    })
}

fn scoring_ui(ui: &mut egui::Ui, d: &mut BasicChordsData) {
    let s = &mut d.scoring;
    ui.label(RichText::new("Targets").small().color(theme::dim()));
    ui.horizontal_wrapped(|ui| {
        checkbox(ui, &mut s.groove, "Groove").on_hover_text("Play the groove's rhythm");
        checkbox(ui, &mut s.piano, "Piano hits")
            .on_hover_text("Play a note with every piano attack");
    });
    ui.add_space(4.0);
    checkbox(ui, &mut s.chord_tones, "Grade chord fit");
    ui.add_enabled_ui(s.chord_tones, |ui| {
        let current = harmony::checker(&d.checker);
        dropdown(ui, "bc_checker", current.name(), |ui| {
            for c in harmony::checkers() {
                selectable_value(ui, &mut d.checker, c.id().to_owned(), c.name())
                    .on_hover_text(c.description());
            }
        });
        ui.add(
            egui::Label::new(
                RichText::new(current.description())
                    .small()
                    .color(theme::dim()),
            )
            .wrap(),
        );
    });
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(
            RichText::new(d.scoring.summary())
                .small()
                .color(theme::dim()),
        )
        .wrap(),
    );
}

fn scale_ui(ui: &mut egui::Ui, d: &mut BasicChordsData) {
    ui.horizontal_wrapped(|ui| {
        dropdown(ui, "bc_root", note_name(d.scale.root as i32), |ui| {
            for pc in 0..12u8 {
                selectable_value(ui, &mut d.scale.root, pc, note_name(pc as i32));
            }
        });
        dropdown(ui, "bc_mode", d.scale.mode.label(), |ui| {
            for m in Mode::ALL {
                selectable_value(ui, &mut d.scale.mode, m, m.label());
            }
        });
    });
}

fn progression_ui(ui: &mut egui::Ui, d: &mut BasicChordsData, run_seed: Option<u64>) {
    let p = &mut d.progression;
    ui.horizontal_wrapped(|ui| {
        ui.radio_value(&mut p.kind, ProgressionKind::Fixed, "Pre-determined");
        ui.radio_value(&mut p.kind, ProgressionKind::SemiRandom, "Semi-random");
        ui.radio_value(&mut p.kind, ProgressionKind::Random, "Random");
    });
    ui.add_space(4.0);
    match p.kind {
        ProgressionKind::Fixed => {
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut p.source, FixedSource::Library, "Common");
                ui.radio_value(&mut p.source, FixedSource::Custom, "Custom");
            });
            match p.source {
                FixedSource::Library => {
                    let name = harmony::library(&p.library).map_or("?", |e| e.1);
                    dropdown(ui, "bc_library", name, |ui| {
                        for (id, name, text) in harmony::LIBRARY {
                            selectable_value(ui, &mut p.library, id.to_owned(), name)
                                .on_hover_text(text);
                        }
                    });
                }
                FixedSource::Custom => {
                    ui.add(
                        text_field(&mut p.custom)
                            .hint("e.g. ii7 V7 I7 vi7")
                            .width(ui.available_width().min(280.0))
                            .font(theme::mono(15.0)),
                    );
                    match harmony::parse_progression(&p.custom, &d.scale) {
                        Ok(specs) => {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!(
                                        "{} chords · repeat a numeral to hold it longer",
                                        specs.len()
                                    ))
                                    .small()
                                    .color(theme::dim()),
                                )
                                .wrap(),
                            );
                        }
                        Err(e) => {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(e.message).small().color(theme::red()),
                                )
                                .wrap(),
                            );
                        }
                    }
                }
            }
        }
        ProgressionKind::SemiRandom => {
            ui.label(
                RichText::new("A new pick on every start, from:")
                    .small()
                    .color(theme::dim()),
            );
            for (id, name, _) in harmony::LIBRARY {
                let mut on = p.pool.is_empty() || p.pool.iter().any(|x| x == id);
                if checkbox(ui, &mut on, name).changed() {
                    if p.pool.is_empty() {
                        p.pool = harmony::LIBRARY.iter().map(|e| e.0.to_owned()).collect();
                    }
                    if on {
                        p.pool.push(id.to_owned());
                    } else {
                        p.pool.retain(|x| x != id);
                    }
                    if p.pool.len() >= harmony::LIBRARY.len() {
                        p.pool.clear();
                    }
                }
            }
        }
        ProgressionKind::Random => {
            ui.horizontal_wrapped(|ui| {
                ui.label("Length");
                ui.add(
                    number_field(&mut p.random_len)
                        .range(2..=16)
                        .suffix("chords"),
                );
            });
            ui.label(
                RichText::new("In key; neighbouring chords share notes.")
                    .small()
                    .color(theme::dim()),
            );
        }
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label("Each chord");
        dropdown(ui, "bc_len", d.chord_length.label(), |ui| {
            for l in ChordLength::ALL {
                selectable_value(ui, &mut d.chord_length, l, l.label());
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Seed");
        ui.add(number_field(&mut d.seed));
        if ui.add(button(format!("{} New", icons::SHUFFLE))).clicked() {
            d.seed = d
                .seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
                >> 16;
        }
        checkbox(ui, &mut d.new_seed_each_start, "New each start");
    });
    if let Some(s) = run_seed.filter(|&s| s != d.seed)
        && ui
            .add(button(format!(
                "{} Keep the last session's chords",
                icons::PUSH_PIN
            )))
            .on_hover_text("Use its seed")
            .clicked()
    {
        d.seed = s;
        d.new_seed_each_start = false;
    }
}

fn width_description(w: f32) -> &'static str {
    match w {
        w if w < 0.05 => "One hand, close voicings",
        w if w < 0.95 => "Mostly one hand, sometimes two",
        w if w < 1.05 => "Two hands: bass and fifth below, chord above",
        w if w < 1.95 => "Two hands, sometimes doubled in octaves",
        _ => "Two hands, doubled in octaves: big and full",
    }
}

fn piano_ui(ui: &mut egui::Ui, d: &mut BasicChordsData) {
    egui::Grid::new("bc_piano")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("Comping");
            let name = comp::PATTERNS
                .iter()
                .find(|p| p.0 == d.comp)
                .map_or("?", |p| p.1);
            dropdown(ui, "bc_comp", name, |ui| {
                for (id, name) in comp::PATTERNS {
                    selectable_value(ui, &mut d.comp, id.to_owned(), name);
                }
            });
            ui.end_row();
            ui.label("Jazziness");
            let mut pct = d.jazziness * 100.0;
            ui.add(
                bg_ui::widgets::controls::slider_field(&mut pct, 0.0..=100.0)
                    .step_by(5.0)
                    .suffix("%"),
            );
            d.jazziness = pct / 100.0;
            ui.end_row();
            ui.label("Width");
            let mut pct = d.width * 100.0;
            ui.add(
                bg_ui::widgets::controls::slider_field(&mut pct, 0.0..=200.0)
                    .step_by(10.0)
                    .suffix("%"),
            )
            .on_hover_text("0% one hand · 100% two hands · 200% two hands with octave doublings; in between mixes them");
            d.width = pct / 100.0;
            ui.end_row();
            ui.label("");
            ui.label(
                RichText::new(width_description(d.width))
                    .small()
                    .color(theme::dim()),
            );
            ui.end_row();
            ui.label("Register");
            bg_ui::widgets::controls::dropdown_value(
                ui,
                "bc_octave",
                &mut d.center_octave,
                2..=6i8,
                |o| match o - harmony::voicing::CENTER_OCTAVE {
                    0 => "Middle (default)".to_owned(),
                    n => format!("{n:+} octave{}", if n.abs() == 1 { "" } else { "s" }),
                },
            );
            ui.end_row();
            ui.label("Humanize");
            let mut pct = d.humanize * 100.0;
            ui.add(
                bg_ui::widgets::controls::slider_field(&mut pct, 0.0..=100.0)
                    .step_by(5.0)
                    .suffix("%"),
            )
            .on_hover_text("Small random changes in timing, chord rolls and dynamics");
            d.humanize = pct / 100.0;
            ui.end_row();
            ui.label("Volume");
            ui.add(egui::Slider::new(&mut d.piano_volume, 0.0..=1.0).show_value(false));
            ui.end_row();
        });
    ui.add(
        egui::Label::new(
            RichText::new(jazz_description(d.jazziness))
                .small()
                .color(theme::dim()),
        )
        .wrap(),
    );
    ui.add_space(6.0);
    egui::Grid::new("bc_keyboard")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            let k = &mut d.keyboard;
            ui.label("Keyboard");
            dropdown(ui, "bc_keys", format!("{} keys", k.keys), |ui| {
                for (n, _, _) in keyboard::SIZES {
                    selectable_value(ui, &mut k.keys, n, format!("{n} keys"));
                }
            });
            ui.end_row();
            ui.label("Key labels");
            dropdown(ui, "bc_labels", k.labels.label(), |ui| {
                for l in KeyLabels::ALL {
                    selectable_value(ui, &mut k.labels, l, l.label());
                }
            });
            ui.end_row();
        });
    let k = &mut d.keyboard;
    checkbox(ui, &mut k.ghost_next, "Outline the next chord");
    checkbox(ui, &mut k.show_bass, "Show my bass note");
    checkbox(
        ui,
        &mut k.hide_chord_names,
        "Hide chord names (read the keyboard)",
    );
}
