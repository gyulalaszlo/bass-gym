//! Preset selection / creation for the active exercise type.
//!
//! - `slot.rs`: an exercise type with its presets, and the store operations.
//! - `bar.rs`: the preset bar, a kit component that emits what to do.
//! - `browser.rs`: the bar's column folder browser, a view.
//! - [`run`] carries out the bar's emissions on a slot and the store.

pub mod bar;
mod browser;
mod slot;

use bg_core::preset::PresetStore;

pub use bar::PresetBar;
pub use slot::ExerciseSlot;

/// Does what the preset bar asked, returning an error to show.
pub fn run(slot: &mut ExerciseSlot, store: &PresetStore, e: bar::Emission) -> Result<(), String> {
    use bar::Emission::*;
    match e {
        Pick(None) => {
            slot.load_default();
            Ok(())
        }
        Pick(Some(id)) => slot.load(id),
        Save => slot.save(store),
        SaveAs(name) => slot.save_as(store, &name),
        Folder(root, folder) => {
            slot.open_folder(root, &folder);
            Ok(())
        }
        Edit {
            name,
            description,
            tags,
        } => slot.edit(store, &name, &description, tags),
        Delete => slot.delete(store),
        Revert => match slot.current {
            Some(id) => slot.load(id),
            None => {
                slot.load_default();
                Ok(())
            }
        },
        Defaults => {
            let data = slot.et.default_preset();
            slot.et.load_preset(data).map_err(|e| format!("{e:#}"))
        }
    }
}
