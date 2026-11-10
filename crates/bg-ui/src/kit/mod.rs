//! The component kit: the small framework every view and component is built on.
//!
//! Tiers (pick the lowest that works):
//! - 0. A leaf control is an `egui::Widget` in `crate::widgets`.
//! - 1. A [`View`] shows `Props` with a `Theme` and emits events, without state.
//! - 2. A [`Component`] adds owned `State`, and a private `Msg` for its own `update`.
//!
//! The full guide is `docs/architecture/ui-components.md`.

mod component;
mod emit;
mod field;
mod rig;
mod split;
mod theme_cache;
mod view;

pub use component::{Component, mount, mount_temp};
pub use emit::Emit;
pub use field::field;
pub use rig::Rig;
pub use split::{Axis, Part, split};
pub use theme_cache::ThemeCache;
pub use view::{View, show};
