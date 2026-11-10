//! Tier 2: [`Component`], a view with owned state and its own update.

use std::fmt::Debug;
use std::hash::Hash;

use egui::Ui;

use super::Emit;

/// A view with persistent `State`, which its parent stores as a named field.
///
/// Messages go two ways and never mix:
/// - `Msg` goes *in*. It's private: the component's own view sends it to its own update.
///   A parent never handles it, or even passes it down.
/// - `Emission` goes *out*, to the parent, which wraps it in a variant of its own `Msg`.
///
/// Arguments always come in the order state, props, theme, then msg / ui, then out.
/// State comes first, standing in for `self`.
pub trait Component {
    type State: Default;
    /// Borrowed, read-only data for this frame.
    type Props<'a>;
    /// How it looks; built from [`crate::theme::Tokens`] by the owner.
    type Theme;
    type Msg;
    type Emission;

    /// Draws and reads input. May only read the state, and sends `Msg`s for changes.
    fn view(
        state: &Self::State,
        props: &Self::Props<'_>,
        theme: &Self::Theme,
        ui: &mut Ui,
        msg: &mut Emit<Self::Msg>,
    );

    /// Applies one message to the state. Pure: no egui and no engine, so it's easy to test.
    fn update(
        state: &mut Self::State,
        props: &Self::Props<'_>,
        msg: Self::Msg,
        out: &mut Emit<Self::Emission>,
    );
}

/// Shows a component under its own id scope, then runs its update on its messages right
/// away, so its state is current and the parent only ever receives emissions.
pub fn mount<C: Component>(
    ui: &mut Ui,
    id: impl Hash + Debug,
    state: &mut C::State,
    props: &C::Props<'_>,
    theme: &C::Theme,
) -> Vec<C::Emission> {
    let mut msgs = Emit::new();
    ui.push_id(id, |ui| C::view(state, props, theme, ui, &mut msgs));
    if !msgs.is_empty() {
        ui.ctx().request_repaint();
    }
    let mut out = Emit::new();
    for m in msgs {
        C::update(state, props, m, &mut out);
    }
    out.into_vec()
}

/// **Migration bridge**: [`mount`] with the state kept in egui's temp memory under `id`,
/// for components whose parent isn't migrated yet and so has nowhere to own the state.
/// Each call site should become a plain [`mount`] once its parent owns the state.
pub fn mount_temp<C>(
    ui: &mut Ui,
    id: impl Hash + Debug,
    props: &C::Props<'_>,
    theme: &C::Theme,
) -> Vec<C::Emission>
where
    C: Component,
    C::State: Clone + Send + Sync + 'static,
{
    let key = ui.id().with(&id).with("kit::mount_temp");
    let mut state: C::State = ui.data_mut(|d| d.get_temp(key)).unwrap_or_default();
    let out = mount::<C>(ui, id, &mut state, props, theme);
    ui.data_mut(|d| d.insert_temp(key, state));
    out
}
