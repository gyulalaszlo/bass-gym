//! Maps transport time to screen x. Shared by views that must line up with each other:
//! the beat grid and its score, or a score row and its waveform.

use egui::Rangef;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeAxis {
    origin_x: f32,
    origin_t: f64,
    /// Points per second.
    pps: f64,
}

impl TimeAxis {
    /// Time `t0..t1` spread over the screen range `x`.
    pub fn new(x: Rangef, t0: f64, t1: f64) -> Self {
        let span = (t1 - t0).max(1e-9);
        Self {
            origin_x: x.min,
            origin_t: t0,
            pps: x.span() as f64 / span,
        }
    }

    /// An axis with time `t` at screen position `x`.
    pub fn anchored(x: f32, t: f64, pps: f64) -> Self {
        Self {
            origin_x: x,
            origin_t: t,
            pps,
        }
    }

    pub fn x(&self, t: f64) -> f32 {
        self.origin_x + ((t - self.origin_t) * self.pps) as f32
    }

    pub fn t(&self, x: f32) -> f64 {
        self.origin_t + (x - self.origin_x) as f64 / self.pps.max(1e-9)
    }

    pub fn pps(&self) -> f64 {
        self.pps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_both_ways() {
        let a = TimeAxis::new(Rangef::new(100.0, 300.0), 10.0, 12.0);
        assert_eq!(a.pps(), 100.0);
        assert_eq!(a.x(10.0), 100.0);
        assert_eq!(a.x(11.5), 250.0);
        assert_eq!(a.t(250.0), 11.5);
    }

    #[test]
    fn anchored_axis_passes_through_its_anchor() {
        let a = TimeAxis::anchored(50.0, 3.0, 20.0);
        assert_eq!(a.x(3.0), 50.0);
        assert_eq!(a.x(2.0), 30.0);
    }
}
