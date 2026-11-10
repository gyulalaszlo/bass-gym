//! Debugging aid: `BASS_GYM_SCREENSHOT=<file.ppm>` saves the window after a few frames
//! and quits (`BASS_GYM_EXERCISE=<id>` picks the exercise shown, `BASS_GYM_PREFS=<page
//! id>` opens the preferences).

use std::path::PathBuf;
use std::time::Instant;

use bg_ui::egui;

pub struct Screenshot {
    path: PathBuf,
    started: Instant,
    requested: bool,
}

impl Screenshot {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("BASS_GYM_SCREENSHOT").map(|p| Self {
            path: PathBuf::from(p),
            started: Instant::now(),
            requested: false,
        })
    }

    /// Call every frame.
    pub fn tick(&mut self, ctx: &egui::Context) {
        let Screenshot {
            path,
            started,
            requested,
        } = self;
        ctx.request_repaint();
        if !*requested && started.elapsed().as_secs_f32() > 2.0 {
            *requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = image {
            let mut ppm = format!("P6 {} {} 255\n", image.size[0], image.size[1]).into_bytes();
            image
                .pixels
                .iter()
                .for_each(|p| ppm.extend_from_slice(&[p.r(), p.g(), p.b()]));
            match std::fs::write(&*path, ppm) {
                Ok(()) => log::info!("screenshot saved to {}", path.display()),
                Err(e) => log::warn!("couldn't save the screenshot: {e}"),
            }
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
        }
    }
}
