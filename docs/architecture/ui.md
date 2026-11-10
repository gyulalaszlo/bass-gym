# UI

egui and eframe 0.36 with the `glow` backend, plus `egui-phosphor` icons (re-exported as
`bg_ui::icons`). `bg-ui` re-exports `egui`. `egui_plot` and `egui_extras` aren't used.

## App shell (`crates/bass-gym/src/app/`)

- `eframe::App::ui` draws the sidebar and a central panel with the tuner, preset bar,
  toolbar and a vertical ScrollArea. `exercise.et.ui(ui, &mut ectx)` runs inside that
  ScrollArea.
- The shell is the root of the kit tree. The sidebar (`sidebar.rs`), the preset bar
  (`presets/bar.rs`, a component; `presets::run` does its operations on the store) and
  the preferences page list (`prefs.rs`) send `Msg`s, and `BassGymApp::update` carries
  them out. `state.rs` is what survives a restart, and `theme.rs` the shell's theme root.
- Preferences open in a separate viewport.
- `persist_egui_memory` is false.

## How state reaches widgets

```rust
pub struct ExerciseCtx<'a> {
    pub engine: &'a mut AudioEngine,
    pub settings: &'a mut SettingsStore,
    pub scoring: &'a ScoringThresholds,
    pub data_dir: Option<&'a Path>,
}
```

- Each `ExerciseType` (`bg-ui/src/exercise.rs`) owns an `Arc<Mutex<TimingShared>>`. The
  worker writes it and the UI locks it per frame.
- Widgets are stateless: they get borrowed data, and mutable view state arrives as
  `&mut` (for example `zoom: &mut usize`).
- Persistent view settings go through `SettingsStore` under an id, as `ViewPrefs` does
  (`bg-ui/src/view.rs`).
- Transient widget state uses `ctx.data` temp storage.

## Painter conventions

New and migrated UI code uses the component kit: see
[ui-components.md](ui-components.md). The conventions below are for the widgets that
aren't migrated yet.

Widgets live one per module in `bg-ui/src/widgets/`, each with a `pub mod` line in
`widgets/mod.rs`.

1. A free fn (`theme::section`), or
   painting into a given rect via `views/performance.rs::Indicator`.
2. `ui.allocate_exact_size(...)`, then `ui.painter_at(rect)`, which clips to the rect.
3. Local `x_of` / `y_of` closures for mapping.
4. All colours from `theme::*`, tinted with `.gamma_multiply(a)`. Upper-case mono labels.
5. Hover details through `resp.on_hover_ui_at_pointer`.
6. Child buttons use `ui.new_child(UiBuilder::new().max_rect(..))`, because `Ui::put`
   moves the cursor back.
7. Most interaction is click only (`views/groove_editor.rs`). `views/marker_editor/` shows
   drag gestures.
8. `is_rect_visible` culling appears in `views/performance.rs` and the review rows.

`button.rs::selectable` and `selectable_value` work as tab-like primitives. Both take a
`button::Theme` through `.theme(&t)`. `.color(c)` is short for
`.theme(&Theme::colored(.., c))`. The
preferences page list uses them; `.centred()` centres the text.

Toolbar menus (`toolbar.rs`): `menu` is an icon button opening a tabbed panel, and
`menu_labelled` the same with the current value as its label. Time signature and key
use the latter, with `choice_grid` (equal-width, centred selectables).

`Toolbar::tap` mounts `views::tap_tempo::TapTempo` through `kit::mount_temp`, so its
taps live in egui temp memory until the exercise owns the toolbar's state.

Shortcuts: Space starts / stops (`Toolbar::transport`) and plays / stops the review;
M toggles monitoring (`app/sidebar.rs`). Both are ignored while a text field has
the keyboard.

## Theme (`bg-ui/src/theme.rs`)

The style is ECAM (Airbus cockpit display). Colour is reserved for meaning: green
normal, cyan selectable / units, amber caution, red fault, magenta targets.

- `Palette`: `bg, panel, raised, line, grid, dim, text, green, amber, red, cyan,
  magenta, button_base`. Backed by a global `RwLock<Palette>`.
- `Tokens` (`theme::tokens()`) holds the palette plus text scale, button rounding and
  tint. Component themes are built from it (see [ui-components.md](ui-components.md)).
  Code that isn't migrated reads the free functions such as `theme::green()`, which
  read the same globals.
- `GradeColors` is the shared grade sub-theme.
- There are 5 presets. `Day` is light, so painters can't assume a black background.
- `theme::grade_color(Grade)` / `GradeColors::get`: Good green, Ok amber, Poor red,
  Missed red at 0.5.
- `mono(size)` and `prop(size)` apply the text scale. Helpers: `caption`, `section`,
  `responsive_columns`, `title_label`.
- Fonts are set in `theme::apply`, which is where any bundled font is registered.

## Beatgrid (`bg-ui/src/views/beatgrid/`)

A tier-1 kit view: `mod.rs` (props), `theme.rs`, `layout.rs` (pure, tested) and
`paint.rs`.

```rust
pub struct Props<'a> {
    pub now: f64, pub bars: &'a [BarPlan], pub matcher: &'a Matcher,
    pub steps_per_beat: u32, pub bars_behind: f64, pub bars_ahead: f64,
    pub chords: &'a [(f64, f64, String)], pub score: Option<LiveScore<'a>>,
    pub lane_height: f32, pub zoom: f64,
}
```

- `kit::show::<Beatgrid>(ui, "grid", &props, &theme)`. The theme comes from a
  `ThemeCache<beatgrid::Theme>` on the exercise. The exercise sets the note lane's
  height (102 in Basic timing, 108 in Basic chords). The theme's `Metrics` add the
  header, chord band and footer, which grow with the text size, so the grid is 150 and
  180 tall at the default size.
- Zoom: `view.rs::ZOOM_STEPS = [0.5, 0.75, 1.0, 1.5, 2.0]`, default index 2, stored in
  `ViewPrefs.grid_zoom` and set in the Display preferences. The step divides the visible
  span.
- The mapping is linear in seconds, so a tempo change alters bar widths on screen.
`layout.rs::layout` gives the visible span `t0 = now - behind`, `t1 = now + ahead`
(`behind = bars_behind * bar_len / zoom`, and likewise ahead) and one `TimeAxis` over it,
shared by the grid and the score.

`score: Some(LiveScore)` adds a staff above the grid on the same axis, built each frame
from the bars in view with `until = now`. `LiveScore::layout` has a fixed range so the
grid doesn't jump. A sounding note appears once its pitch arrives.
`ViewPrefs.show_score` and `.score_size` set it, in the Display preferences. The
Results score has its own `.results_score_size`.

`bar_len` is the duration of the current bar. Nothing scrolls: everything repaints from
`now` with `x_now` fixed, and the exercise calls `request_repaint()` while running.
After a session `now = last_time(...)`, and before one a `preview()` is shown.

Draw order (`paint.rs`, one function per step):
1. The live score, if shown. Then the grid's background and frame.
2. Per visible bar: count-in fill, bar number and BPM, "CLICK OFF" band if muted, solid
   bar line.
3. Dashed beat and sub-step lines. These are the grid, not expected notes.
4. Chord band, when `chords` isn't empty.
5. Expected notes as rectangles (height 20, or 30 if accented). Undecided: `text`
   outline, `cyan` while active. Decided: grade colour at 0.18 fill plus a stroke.
6. Played notes as brackets in their grade colour, with a timing-error line. Extra notes
   sit in a lane above, in red. In `Separate` mode a harmony dot appears above the note.
7. Magenta now line with a triangle.

Culling relies on time-sorted vectors.

## Score, waveform and marker editor

- `widgets/time_axis.rs::TimeAxis` maps time to x. Anything that must line up shares one.
- `widgets/score.rs::ScoreView` paints `bg_core::score` bars along a `TimeAxis` in a
  `score::Theme`'s colours, with
  `StaffLayout` for vertical sizes. Notation glyphs come from Bravura
  (`assets/fonts/`, SIL OFL), a separate `FontFamily::Name(MUSIC_FONT)` set up in
  `theme::apply`. `Draw::glyph` places glyphs by their baseline, since SMuFL fonts have
  extreme line metrics; font size is four staff spaces.
- `widgets/waveform.rs` paints any `PeakSource` with auto vertical zoom and an optional
  log scale.
- `views/marker_editor/` is a generic tier-2 component: it takes a `PeakSource`,
  read-only overlays, the owner's `Marker`s, a snap function and a `new_marker` factory,
  and holds no session types. `add_remove: false` fixes the marker set, for loop points.
  The owner keeps `EditorState` and the markers. The editor emits `Add` / `Move` /
  `Remove` / `Relabel`, which `apply` makes to a `Vec`. Its `update` needs no props, so
  the review (a sub-program) routes its messages through `review::Msg::Editor`.
- `input.rs` reads egui into a plain `Input`, and `interpret` (pure, tested) turns it into
  messages, following the gesture and selection within the frame (a drag can start and
  move in one). Keep `Response` calls outside `ui.input(..)`: both lock the context.
- Editor drags work like Ableton: `WaveView::navigate` keeps the point where the drag
  began under the pointer while vertical movement zooms (down zooms in). Labels wrap and
  grow, and the area under the waveform grows with the tallest one in view.
- `Header` draws something above the waveform on the same axis; the review uses it for
  the score. New markers and clicked labels take keyboard focus (`focus_label` asks for
  two frames, as the click can take focus away on its own frame).
- `ScoreView` paints chord marks first, then staff, then notes, so marks stay a hint.
- `Lead { clef, key, sig }` draws at the start of a review row (key on every row, clef
  on the first). The live score and the editor header have no lead; `paint_fixed_lead`
  covers their left end with the clef and key while notes scroll behind.

## Results dialog (`bg-ui/src/views/results/`)

- `results_dialog(ctx, open, title, add)` is an `egui::Modal` with a title, a vertical
  ScrollArea and a Close button. Width clamps to 320-960 pt.
- It closes on Close, Space, Esc or a click outside. The Space check runs after `add`,
  so content can consume Space first, as `SessionReview` does for play / stop.
- `modal_open` keeps the toolbar's Space-to-start from firing while it's open.
- `results_dialog(ctx, &theme.dialog, open, title, tab, add)` is the modal container.
  The `Stats` view (`results::Props { stats, thresholds, tables }`) draws summary
  groups, `accuracy_timeline` (`timeline.rs`), three `views::histogram`s, the grade and
  tempo tables, and an exercise-specific `tables` callback. `block(ui, theme, ..)`
  captions an exercise's table. All of it reads `results::Theme`.
- It's called from each exercise's `view/results.rs`. Basic chords' closure adds "Per
  chord" (`view/per_chord.rs`, rows computed in `props.rs::per_chord`). Its HIT and IN
  CHORD columns hide when the preset scores no targets or no chord fit.
- Passing a `ResultsTab` shows the tabs. Score + Audio widens the dialog to 1400 pt.
- While a text field or popup is active, Esc and clicks outside don't close the dialog.
- `results_button` reopens it. Any config edit or preset load clears `has_results`,
  after which results can't be reached.
- `matcher.stats()` is recomputed every frame while open.

## Session review (`bg-ui/src/views/review/`)

The Score + Audio tab, as a kit sub-program (see
[ui-components.md](ui-components.md)): the exercise keeps the `Review` in its model, so
selection, markers and playback survive closing the dialog. It's built from a
`ReviewInput` snapshot (`Msg::ReviewReady`, sent by the exercise's `sync::review` once
`!session_running` and the recording has arrived), and released with `has_results`.

The exercise routes it:
- wraps `review::Msg` in `Msg::Review` and calls `review::update`;
- wraps `review::Effect` in `Effect::Review` and calls `review::run`;
- calls `review::sync` before the dialog and sends `review::Msg::Tick` after it.

Files:
- `update.rs`: pure, with tests for playback, queueing and selection.
- `effects.rs`: replay start / stop / release, the replay options, the export thread and
  opening the folder.
- `sync.rs`: the clock while a replay runs, and polling the export.
- `view/`: controls, report row, score rows and the marker editor.
- `theme.rs`: every colour and size.

- Rows of whole bars: one shared `TimeAxis` per row for score and waveform. Staff
  spacing is `ViewPrefs.results_score_size`, passed in `review::Props` each frame, and
  `bars_per_row` scales with it.
- "Back to score" sets `reveal` to the editor view's centre; `rows` scrolls that row
  into view and sends `Revealed`. A short last row keeps the scale of full rows. Rows
  are culled with `is_rect_visible`.
- `overlays` draws string noise (`Matcher.noise`) as faint dotted spans on the waveform.
- Click the score to select a bar and play from there to the end of the session: from
  the recorded note under the click, or else the bar's start. Shift extends the selection
  and doesn't play. Click the waveform to open `MarkerEditor` around that time.
- Stopping a replay isn't immediate, so a click during playback is `queued`, and `Tick`
  starts it once the worker has stopped.
- `MarkerEditor` isn't migrated: `view/editor.rs` runs it on copies of the markers and
  `EditorState` (`kit::edited`) and sends `Msg::Editor` when they change.
- The tab scrolls itself below its toolbar: `results_dialog` gives it a bounded `Ui`
  rather than the scroll area the Statistics tab gets.
- Export runs `bg_report::export` on a thread (`Effect::Export`), and `sync` polls the
  result. `ReviewInput.settings` comes from `report_settings`, taken when the session
  ended.
- `sync` and `Tick` send messages only while a replay runs or the engine's running
  state changes, so an idle review doesn't keep the app repainting.

## Settings and persistence

- `bg-ui/src/settings.rs::SettingsStore` is one TOML file, `<config dir>/settings.toml`,
  with one table per module id. `get<T>(id)` returns defaults if the table is missing or
  invalid. `set` bumps the revision. `flush` writes atomically, called from
  `refresh_settings` and `on_exit`.
- Scoring tolerances are global (`pages/scoring.rs`), but what is scored belongs to
  the preset. Note length is a checkbox in the groove panel (`SessionConfig.score_length`).
  Basic chords' "Scoring" section has the target sources (groove, piano hits), chord
  fit and the checker.
- Other stores: presets in `<config dir>/presets` (exercise config only) and
  `state.json` for `AppState`.
- Changing anything in `AudioConfig` reopens the devices, so unrelated settings should
  get their own table.

To add a setting:
1. Write a struct with `#[serde(default)]` and `Default`, add an id constant in
   `pages/mod.rs`, and load it in `app/mod.rs`.
2. Add a section to a page in `crates/bass-gym/src/pages/`. The section is a view whose
   props are the settings struct and whose emission is the edited struct, built with
   `kit::edited`.
3. In the page's `ui`, for each emission, `settings.set` it and make the engine call
   that applies it (`ctx.engine.set_x()`), only when it changed. `pages/audio/mod.rs`
   and `pages/latency.rs` are the examples with engine calls.

## Visual checks

There are no snapshot tests. Use the `BASS_GYM_*` variables in
[README.md](README.md) to take a screenshot. For interaction, `kit::Rig` runs UI code in a
real `egui::Context` fed with input events. `views/marker_editor/tests.rs` shows a
harness that applies messages each frame as an owner would. Run the binary directly rather than through `cargo run` with `HOME` changed,
because rustup reads its toolchain from `HOME`.
