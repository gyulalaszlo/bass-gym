mod app;
mod menu;
mod pages;
mod presets;
mod soundfonts;

/// `BASS_GYM_SIZE=<w>x<h>`, a fixed window size for screenshots.
fn forced_size() -> Option<[f32; 2]> {
    std::env::var("BASS_GYM_SIZE").ok().and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some([w.parse().ok()?, h.parse().ok()?])
    })
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let forced = forced_size();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("bass-gym")
            .with_inner_size(forced.unwrap_or([1280.0, 860.0]))
            .with_min_inner_size([900.0, 600.0]),
        // The window's position and size are restored on the next launch (not with a forced size).
        persist_window: forced.is_none(),
        persistence_path: directories::ProjectDirs::from("", "", "bass-gym")
            .map(|d| d.config_dir().join("window.ron")),
        ..Default::default()
    };
    eframe::run_native(
        "bass-gym",
        options,
        Box::new(|cc| Ok(Box::new(app::BassGymApp::new(cc)))),
    )
}
