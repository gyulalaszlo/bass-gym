//! Tier 1: [`View`], a component without persistent state.

use std::fmt::Debug;
use std::hash::Hash;

use egui::Ui;

use super::Emit;

/// Display plus input, without state of its own. The implementing type is a zero-sized
/// marker (`pub struct Beatgrid;`); the data lives in `Props` and the look in `Theme`.
///
/// A view has no `update`, so nothing flows into it but its props: it only has an
/// `Emission`. One that never emits uses [`std::convert::Infallible`].
pub trait View {
    /// Borrowed, read-only data for this frame.
    type Props<'a>;
    /// How it looks; built from [`crate::theme::Tokens`] by the owner.
    type Theme;
    /// Events for the parent.
    type Emission;

    fn view(
        props: &Self::Props<'_>,
        theme: &Self::Theme,
        ui: &mut Ui,
        out: &mut Emit<Self::Emission>,
    );
}

/// Shows a view under its own id scope and returns what it emitted.
pub fn show<V: View>(
    ui: &mut Ui,
    id: impl Hash + Debug,
    props: &V::Props<'_>,
    theme: &V::Theme,
) -> Vec<V::Emission> {
    let mut out = Emit::new();
    ui.push_id(id, |ui| V::view(props, theme, ui, &mut out));
    out.into_vec()
}
