//! Where SoundFont instruments are looked for.

use std::path::PathBuf;

/// The user's own SoundFonts: `<config dir>/soundfonts`.
pub fn user_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "bass-gym").map(|d| d.config_dir().join("soundfonts"))
}

/// Search order: the user's folder, then the ones shipped with the app (next to the
/// executable, in a macOS bundle's `Resources`), then the source tree for `cargo run`.
pub fn dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = user_dir().into_iter().collect();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        dirs.push(exe_dir.join("soundfonts"));
        dirs.push(exe_dir.join("../Resources/soundfonts"));
    }
    dirs.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/soundfonts"
    )));
    dirs
}

/// Registers every SoundFont found; call before the audio engine is created.
pub fn register() {
    let dirs = dirs();
    let n = bg_sound::register_soundfonts(&dirs);
    log::info!("{n} SoundFont instrument(s) found");
}
