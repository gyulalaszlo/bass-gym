//! Showing folders in the system's file manager.

use std::path::Path;

pub fn open_folder(dir: &Path) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(cmd).arg(dir).spawn() {
        log::warn!("couldn't open {}: {e}", dir.display());
    }
}
