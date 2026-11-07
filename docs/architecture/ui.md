# UI

egui and eframe 0.36 with the `glow` backend, plus `egui-phosphor` icons (re-exported as
`bg_ui::icons`). `bg-ui` re-exports `egui`. `egui_plot` and `egui_extras` aren't used.

## App shell (`crates/bass-gym/src/app.rs`)

- `eframe::App::ui` draws the sidebar and a central panel with the tuner, preset bar,
  toolbar and a vertical ScrollArea. `exercise.et.ui(ui, &mut ectx)` runs inside that
  ScrollArea.
- Preferences open in a separate viewport.
- `persist_egui_memory` is false.

## How state reaches widgets

```rust
pub struct ExerciseCtx<'a> {
    pub engine: &'a mut AudioEngine,
    pub settings: &'a mut SettingsStore,
    pub scoring: &'a ScoringThresholds,
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

Widgets live one per module in `bg-ui/src/widgets/`, each with a `pub mod` line in
`widgets/mod.rs`.

1. A struct of borrowed data with `show(&self, ui, height_or_size)` (`Beatgrid`,
   `Histogram`, `Keyboard`), a free fn (`accuracy_timeline`, `groove_editor`), or
   painting into a given rect via `performance.rs::Indicator`.
2. `ui.allocate_exact_size(...)`, then `ui.painter_at(rect)`, which clips to the rect.
3. Local `x_of` / `y_of` closures for mapping.
4. All colours from `theme::*`, tinted with `.gamma_multiply(a)`. Upper-case mono labels.
5. Hover details through `resp.on_hover_ui_at_pointer`.
6. Child buttons use `ui.new_child(UiBuilder::new().max_rect(..))`, because `Ui::put`
   moves the cursor back.
7. Most interaction is click only (`groove_editor`). `marker_editor` shows drag gestures.
8. `is_rect_visible` culling appears only in `performance.rs`.

`button.rs::selectable` and `selectable_value` work as tab-like primitives. The
preferences page list uses them.

## Theme (`bg-ui/src/theme.rs`)

The style is ECAM (Airbus cockpit display). Colour is reserved for meaning: green
normal, cyan selectable / units, amber caution, red fault, magenta targets.

- `Palette`: `bg, panel, raised, line, grid, dim, text, green, amber, red, cyan,
  magenta, button_base`. Read through free functions such as `theme::green()`, backed
  by a global `RwLock<Palette>`.
- There are 5 presets. `Day` is light, so painters can't assume a black background.
- `theme::grade_color(Grade)`: Good green, Ok amber, Poor red, Missed red at 0.5.
- `mono(size)` and `prop(size)` apply the text scale. Helpers: `caption`, `section`,
  `responsive_columns`, `title_label`.
- Fonts are set in `theme::apply`, which is where any bundled font is registered.

## Beatgrid (`bg-ui/src/widgets/beatgrid.rs`)

```rust
pub struct Beatgrid<'a> {
    pub now: f64, pub bars: &'a [BarPlan], pub matcher: &'a Matcher,
    pub steps_per_beat: u32, pub bars_behind: f64, pub bars_ahead: f64,
    pub chords: &'a [(f64, f64, String)],
}
```

- `show_zoomable` adds a `ZOOM_COLUMN` (26 pt, plus 2 pt gap) button column and is what
  all call sites use. `show` has no callers. Heights are 150 in Basic timing and 180 in
  Basic chords.
- Zoom: `view.rs::ZOOM_STEPS = [0.5, 0.75, 1.0, 1.5, 2.0]`, default index 2, stored in
  `ViewPrefs.grid_zoom`. The step divides the visible span.
- The mapping is linear in seconds, so a tempo change alters bar widths on screen:

```rust
let behind = self.bars_behind * bar_len / zoom;
let ahead  = self.bars_ahead  * bar_len / zoom;
let x_now  = rect.left() + rect.width() * (behind / (behind + ahead)) as f32;
let pps    = rect.width() as f64 / (behind + ahead);
let x_of   = |t: f64| x_now + ((t - self.now) * pps) as f32;
```

`bar_len` is the duration of the current bar. Nothing scrolls: everything repaints from
`now` with `x_now` fixed, and the exercise calls `request_repaint()` while running.
After a session `now = last_time(...)`, and before one a `preview()` is shown.

Draw order:
1. Background and frame.
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
- `widgets/score.rs::ScoreView` paints `bg_core::score` bars along a `TimeAxis`, with
  `StaffLayout` for vertical sizes. Notation glyphs come from Bravura
  (`assets/fonts/`, SIL OFL), a separate `FontFamily::Name(MUSIC_FONT)` set up in
  `theme::apply`. `Draw::glyph` places glyphs by their baseline, since SMuFL fonts have
  extreme line metrics; font size is four staff spaces.
- `widgets/waveform.rs` paints any `PeakSource` with auto vertical zoom and an optional
  log scale.
- `widgets/marker_editor.rs::MarkerEditor` is generic: it takes a `PeakSource`, read-only
  overlays, editable `Marker`s, a snap function and a `new_marker` closure, and holds no
  session types. `add_remove: false` fixes the marker set, for loop points. The owner
  keeps `EditorState`. It's the first widget using `Sense::drag`.

## Results dialog (`bg-ui/src/widgets/results.rs`)

- `results_dialog(ctx, open, title, add)` is an `egui::Modal` with a title, a vertical
  ScrollArea and a Close button. Width clamps to 320-960 pt.
- It closes on Close, Space, Esc or a click outside. The Space check runs after `add`,
  so content can consume Space first.
- `modal_open` keeps the toolbar's Space-to-start from firing while it's open.
- `results(ui, stats, th, tables)` draws summary groups, `accuracy_timeline`, three
  histograms, the grade and tempo tables, and an exercise-specific `tables` closure.
- It's called from `basic_timing.rs` and `basic_chords/mod.rs`. Basic chords' closure
  adds "Per chord" and borrows all of `self`.
- `results_button` reopens it. Any config edit or preset load clears `has_results`,
  after which results can't be reached.
- `matcher.stats()` is recomputed every frame while open.

## Settings and persistence

- `bg-ui/src/settings.rs::SettingsStore` is one TOML file, `<config dir>/settings.toml`,
  with one table per module id. `get<T>(id)` returns defaults if the table is missing or
  invalid. `set` bumps the revision. `flush` writes atomically, called from
  `refresh_settings` and `on_exit`.
- Other stores: presets in `<config dir>/presets` (exercise config only) and
  `state.json` for `AppState`.
- Changing anything in `AudioConfig` reopens the devices, so unrelated settings should
  get their own table.

To add a setting: a struct with `#[serde(default)]` and `Default`, an id constant in
`pages.rs`, a load in `app.rs`, a section in a `PreferencePage`, then the get -> edit ->
set -> `ctx.engine.set_x()` pattern that `LatencyPage` uses.

## Visual checks

There are no snapshot tests. Use the `BASS_GYM_*` variables in
[README.md](README.md) to take a screenshot.
