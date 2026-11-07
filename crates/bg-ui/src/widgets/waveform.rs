//! Paints audio as a min/max waveform along a [`TimeAxis`]. Works on any [`PeakSource`].

use egui::{Color32, Mesh, Painter, Rect, Shape, pos2};

use crate::widgets::time_axis::TimeAxis;

/// Audio addressed by time, in whatever unit the view's axis uses.
pub trait PeakSource {
    /// The time range audio exists for.
    fn range(&self) -> (f64, f64);
    /// Lowest and highest sample over `from..to`, if there's audio there.
    fn peak(&self, from: f64, to: f64) -> Option<(f32, f32)>;
}

/// Lowest level shown on the log scale, in dB.
const LOG_FLOOR_DB: f32 = -60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveStyle {
    /// Show levels in dB, which makes quiet tails readable.
    pub log: bool,
    pub color: Color32,
}

/// A sample's height (-1..1) on the chosen scale.
pub fn scaled(v: f32, log: bool) -> f32 {
    if !log {
        return v;
    }
    let level = 20.0 * v.abs().max(1e-9).log10();
    v.signum() * (1.0 - level / LOG_FLOOR_DB).clamp(0.0, 1.0)
}

/// Sets how tall audio with these peaks is drawn, so the loudest sample nearly fills the
/// height. Near-silence isn't blown up.
pub fn auto_zoom(peak: f32) -> f32 {
    1.0 / peak.max(0.01)
}

/// Paints the visible part of `source` in `rect`, scaled to fill the height.
pub fn paint(p: &Painter, rect: Rect, axis: TimeAxis, source: &dyn PeakSource, style: WaveStyle) {
    let px = 1.0_f32;
    let columns = (rect.width() / px).ceil() as usize;
    let mut peaks = Vec::with_capacity(columns);
    let mut loudest = 0.0_f32;
    for c in 0..columns {
        let x = rect.left() + c as f32 * px;
        let found = source.peak(axis.t(x), axis.t(x + px));
        if let Some((lo, hi)) = found {
            loudest = loudest.max(lo.abs()).max(hi.abs());
        }
        peaks.push((x, found));
    }
    let zoom = auto_zoom(loudest);
    let mid = rect.center().y;
    let half = rect.height() / 2.0 - 1.0;
    let mut mesh = Mesh::default();
    for (x, found) in peaks {
        let Some((lo, hi)) = found else { continue };
        let top = mid - scaled((hi * zoom).clamp(-1.0, 1.0), style.log) * half;
        let bottom = mid - scaled((lo * zoom).clamp(-1.0, 1.0), style.log) * half;
        mesh.add_colored_rect(
            Rect::from_min_max(
                pos2(x, top.min(mid - 0.5)),
                pos2(x + px, bottom.max(mid + 0.5)),
            ),
            style.color,
        );
    }
    p.add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_scale_is_unchanged() {
        assert_eq!(scaled(0.25, false), 0.25);
    }

    #[test]
    fn log_scale_lifts_quiet_levels_and_keeps_the_sign() {
        assert_eq!(scaled(1.0, true), 1.0);
        assert!((scaled(0.1, true) - 2.0 / 3.0).abs() < 1e-5); // -20 dB of 60.
        assert!((scaled(-0.1, true) + 2.0 / 3.0).abs() < 1e-5);
        assert_eq!(scaled(0.0, true), 0.0);
    }

    #[test]
    fn auto_zoom_fills_the_height_but_leaves_silence_alone() {
        assert_eq!(auto_zoom(0.5), 2.0);
        assert_eq!(auto_zoom(0.0), 100.0);
    }
}
