# UI components

How UI code is structured: the component kit (`bg-ui/src/kit/`), its tiers, where files
go, and how themes work. The UI is being migrated to this a piece at a time. Code that
isn't migrated yet follows the older conventions in [ui.md](ui.md).

The goal: for every component it should be clear what data it uses, and layout,
painting, input and state changes should be kept apart. The component tree should be
built the same way everywhere, so that a layout / wiring DSL can target it later.

## Tiers

Pick the lowest tier that works. Most components don't need state.

| Tier | What | Contract | Lives in |
|---|---|---|---|
| 0 Widget | Leaf control | `impl egui::Widget`, returns a `Response` | `widgets/` |
| 1 View | Display plus input, no state | `Props` + `Theme` in, `Emission` out | `views/` |
| 2 Component | A view with owned `State` and its own `update` | `State`, `Props`, `Theme`, `Msg` in, `Emission` out | `views/` |
| 3 Program | A screen: model, sync, update, effects | `kit::Program` + `dispatch` | the exercise's folder |

```rust
pub trait View {
    type Props<'a>; type Theme; type Emission;
    fn view(props: &Self::Props<'_>, theme: &Self::Theme, ui: &mut Ui, out: &mut Emit<Self::Emission>);
}

pub trait Component {
    type State: Default; type Props<'a>; type Theme; type Msg; type Emission;
    fn view(state: &Self::State, props: &Self::Props<'_>, theme: &Self::Theme, ui: &mut Ui, msg: &mut Emit<Self::Msg>);
    fn update(state: &mut Self::State, props: &Self::Props<'_>, msg: Self::Msg, out: &mut Emit<Self::Emission>);
}
```

Arguments always come in the order state, props, theme, then msg / ui, then out. State
comes first, standing in for `self`.

## The data a component uses

- **`Props<'a>`**: borrowed, read-only data for this frame. Its parent builds it.
- **`Theme`**: how it looks. It's a separate argument, not a Props field (see Themes).
- **`State`**: what the component remembers between frames. The parent stores it as a
  named field, so it can be seen and tested. Don't hide it in `ctx.data`. The one
  exception is `kit::mount_temp`, a migration bridge (see below).
- **`Msg`** goes *in*. It's private to the component: its view sends a `Msg` and its own
  `update` handles it. A parent never handles a child's `Msg`, or passes it down.
- **`Emission`** goes *out*, to the parent. A view that never emits uses
  `std::convert::Infallible`.

`view` only reads. It reports changes as messages and never writes data or calls the
engine.

## Building the tree

```rust
// in the parent's view
let out_grid = kit::show::<Beatgrid>(ui, "grid", &grid_props, &theme.grid);
let out_tap = kit::mount::<TapTempo>(ui, "tap", &mut state.tap, &tap_props, &theme.tap);
out.extend_map(out_tap, Msg::Tap);

// in the parent's update: the child's Emission arrives in one of the parent's Msgs
Msg::Tap(tap_tempo::Emission::Tempo(bpm)) => model.bpm = bpm,
```

- `show` runs a view in its own id scope and returns its emissions.
- `mount` also runs the component's `update` on its messages straight away, so its state
  is current when `mount` returns, and the parent only sees emissions. It requests a
  repaint when there were messages.
- Every child appears once in the parent's view and once in the parent's `Msg`.
  Visibility is an ordinary `if` around the one `show` / `mount` call.

### Forms

`kit::field` lets form code stay short without writing in place. It runs the widget
on a copy of the value, and if the copy changed it sends a message with the new value:

```rust
field(ui, &props.bpm, out, Msg::Bpm, |ui, v| ui.add(number_field(v).range(20.0..=400.0)));
field(ui, &props.click, out, Msg::Click, |ui, v| checkbox(ui, v, "Click"));
```

### Migration bridge

`kit::mount_temp` is `mount` with the state kept in egui temp memory. Use it only where
the parent isn't migrated yet and so has nowhere to own the state. Today the only user is
`Toolbar::tap`. Each one becomes a plain `mount` once its parent owns the state.

## Where files go

The path tells you the role.

```
bg-ui/src/
  kit/                 the framework: emit, view, component, field, split, theme_cache, rig
  widgets/             tier 0 leaf controls (and code not migrated yet)
  views/               tier 1 and 2
    tap_tempo.rs       a small component: one file
    beatgrid/          a big one: one folder with fixed file names
      mod.rs           Props, State, Msg, Emission, the trait impl (wiring only)
      theme.rs         Theme and its constructors
      layout.rs        pure: fn layout(&Props, Rect) -> Layout, with tests
      paint.rs         drawing only: reads props, theme and layout
      input.rs         egui responses -> Msg (when interactive)
      update.rs        pure state changes (tier 2)
```

If a file name is missing, the component doesn't have that part. For example, the
beatgrid has no `input.rs` because nothing in it is interactive.

Layout helpers: `kit::split(rect, Axis, gap, [Part; N]) -> [Rect; N]`. Fixed sizes are
taken first, and the rest is shared by weight. `Part::NONE` is for a hidden part.

## Themes

There are two layers.

1. **`theme::Tokens`** are the global design tokens: the palette (5 presets plus custom),
   the text scale, the shared `Spacing` (item spacing, button padding, interact height)
   and the button look. They come from the display preferences, so they have to be
   global. `theme::tokens()` reads the current ones. Only Theme constructors use them.
   `theme::apply` copies the same `Spacing` into egui's style, so egui widgets and kit
   components agree.
2. **A `Theme` per component** is a plain struct in its own file, built from `&Tokens`.
   A parent's theme holds its children's themes, so the parent decides how each group of
   children looks:

```rust
pub struct Theme { pub white_key: button::Theme, pub black_key: button::Theme }
impl Theme {
    pub fn new(t: &Tokens) -> Self {
        Self { white_key: button::Theme::normal(t), black_key: button::Theme::colored(t, t.text()) }
    }
}
impl From<&Tokens> for Theme { fn from(t: &Tokens) -> Self { Self::new(t) } }
```

- The owner of a theme tree's root keeps a `kit::ThemeCache<Theme>` field. Its `get()`
  rebuilds the theme only when the tokens change. It has a `Default` for themes with
  `From<&Tokens>`. The two exercises hold one for their beat grid.
- **Sizes, margins, paddings and gaps belong to the theme too**, so a differently themed
  copy of a component can change them like any colour (e.g. `button::Theme::padding`,
  `min_size`, or the beat grid's `Metrics`). Sizes are in points, so the UI zoom scales
  them on its own. Sizes that hold text follow the text size as well: build them with
  `Tokens::em` / `em2`, or take them from `Tokens::spacing`. Only proportions intrinsic
  to a shape, such as a dash length, stay as constants in paint code.
- When a component's height depends on the theme, its Props ask for the part that's
  the parent's choice (the beat grid's `lane_height`), and the component adds its
  themed header and footer around it.
- Painters read only their `Theme`. A `theme::green()` call in migrated paint code is a
  bug. Children that aren't migrated yet, such as `ScoreView` inside the beat grid,
  still read the globals.
- Shared sub-themes go in `theme.rs`. So far that's `GradeColors`.
- Leaf widgets take a theme through a builder, e.g. `button(..).theme(&t)`, where `t` is
  a `button::Theme`. Without one they use the default for the current tokens.

## Programs (tier 3)

A program is the root of a screen, such as an exercise. It has effects instead of
emissions:

```rust
pub trait Program {
    type Model; type Msg; type Effect; type Ctx: ?Sized;
    fn update(model: &mut Self::Model, msg: Self::Msg, fx: &mut Emit<Self::Effect>);
    fn run(effect: Self::Effect, ctx: &mut Self::Ctx, out: &mut Emit<Self::Msg>);
}
```

`kit::dispatch::<P>(model, ctx, msgs)` updates the model with each message and runs
each effect. Messages that come back from effects are handled in the same dispatch.

One frame:
1. **sync** reads the outside world (engine, worker state, settings) and turns it into
   messages. It's the only code that reads it. A per-frame snapshot (`Frame`) is a
   message too, so even "the session just ended" is decided in `update`.
2. **props** derives what each view region shows, once.
3. **view** regions send messages. The shell gives an exercise several regions
   (toolbar, notices, main), so each region dispatches its own messages.
4. **update** is pure, so test it like any function. **effects** is the only code that
   calls the engine. An effect's result comes back as a message (`Msg::Started(id)`).

Files: `crates/bg-exercises/src/basic_timing/`:
- `mod.rs`: the glue to `ExerciseType`
- `model.rs`, `msg.rs`, `sync.rs`, `props.rs`, `update.rs`, `effects.rs`, `theme.rs`
- `view/<region>.rs`

A region is a plain function with the usual argument order, `(props, theme, surface,
out)`. The surface is a `Ui` or the `Toolbar` builder.

A settings section's props name exactly the fields it shows, and its `Emission` has one
variant per thing it edits (`progression::Emission::ChordLength(..)`). The program's
`update` applies them, and any real change drops the results.

After a dispatch that handled messages, the exercise requests a repaint, because the
views showed the state from before them.

### Sub-programs

A child with effects of its own, such as the session review (replay, export), can't be
a component, because only a program runs effects. It's a **sub-program** instead:
`model`, `msg`, `update`, `effects` and `sync` like a program, plus a view whose
`Emission` is its own `Msg`. Its owner routes it, as Elm does with nested programs:
`Msg::Review(review::Msg)` goes to `review::update`, `Effect::Review(review::Effect)`
goes to `review::run`, and the owner calls `review::sync` before showing it. Here a
child's `Msg` does pass through its parent, because the parent has to route its effects.

The same goes for a component inside a program's view, since program views only read:
the program shows it with the component's plain `view` function and routes its `Msg` to
the component's `update`. That's why a component's `update` shouldn't need props when it
can be avoided (the marker editor's doesn't). Emissions are then applied by the owner,
e.g. `marker_editor::apply`.

Bridges, while children aren't migrated:
- Widgets that edit in place (`session_toolbar`, `session_panel`) get a copy of the
  value. If they report a change, the copy is sent as a message.
- (The marker editor used to be bridged like this. It's now a component.)

## Testing

- `layout.rs` and `update.rs` are pure, so test them directly (`beatgrid/layout.rs`,
  `tap_tempo.rs`).
- For interaction, `kit::Rig` runs UI code in a real `egui::Context`. It has `frame`,
  `click`, `double_click`, `drag`, `type_text` and `wait`. The UI function is a
  `&mut dyn FnMut(&mut Ui)`; use `Cell` / `RefCell` for anything the test reads
  between calls (`kit/field.rs` tests).
- To test a component the way its owner runs it, apply its messages and emissions each
  frame (`views/marker_editor/tests.rs`).

## Migration status

Done:
- Phase 1:
  - `kit`
  - `Tokens` (spacing included)
  - `button::Theme`
  - `views::beatgrid` (tier 1)
  - `views::tap_tempo` (tier 2, bridged through `mount_temp`)
- Phase 2: Basic timing as a tier-3 program (`kit::Program`)
- Phase 3: Basic chords as a program. Its settings sections (`view/config/`) are views
  over their slice of the data, with typed emissions built with `kit::field` and
  `kit::edited`. The strip, per-chord table and status are views with their own themes.
  `theme::FormTheme` is the shared look of settings sections.
- Phase 4: the session review as a sub-program (`views/review/`). Its engine calls are
  effects, and its view only reads.
- The marker editor as a tier-2 component (`views/marker_editor/`), routed through the
  review.

- Preference pages (`bass-gym/src/pages/`, and Basic chords' page): sections are views
  over their settings struct, and the page glue stores and applies changes.
- `widgets/score.rs` takes a `score::Theme` (`ScoreView::paint(p, rect, theme)`,
  `paint_fixed_lead(.., theme)`), held by the beat grid's and the review's themes.
- The tuner (`views/tuner.rs`; `Theme::tune_color` replaces `TunerPrefs::color`).
- The keyboard and groove editor (`views/keyboard.rs`, `views/groove_editor.rs`, which
  emits the edited groove).
- The performance strip, histograms and results dialog (`views/performance.rs`,
  `views/histogram.rs`, `views/results/`).
- The app shell (`bass-gym/src/app/`): the sidebar, the preset bar (a component) and
  the preferences page list are views, and the root's `update` carries out their
  messages.

Still bridged or reading the global theme (migrate as they're touched):
- The toolbar API (`widgets/toolbar.rs`): tap tempo lives in `kit::mount_temp` until the
  toolbar gives its owner the state.
- `session_toolbar` / `session_panel` edit in place, and their callers pass a copy.
- Still painting with `theme::*`: the small helpers in `theme.rs` (`caption`, `section`, `title_label`)
  and `controls.rs`. (`waveform::paint` takes its colour from the caller's
  `WaveStyle`.)

Next: the layout / wiring DSL.
