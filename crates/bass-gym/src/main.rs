mod app;
mod pages;
mod presets;
mod soundfonts;

/// 1280×860, or `BASS_GYM_SIZE=<w>x<h>`.
fn window_size() -> [f32; 2] {
    std::env::var("BASS_GYM_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        })
        .unwrap_or([1280.0, 860.0])
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_title("bass-gym").with_inner_size(window_size()).with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("bass-gym", options, Box::new(|cc| Ok(Box::new(app::BassGymApp::new(cc)))))
}
