use std::f32::consts::PI;

/// RBJ biquad (direct form I).
#[derive(Clone, Debug, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    pub fn lowpass(sr: f32, freq: f32, q: f32) -> Self {
        let w = 2.0 * PI * freq / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - c) / 2.0 / a0,
            b1: (1.0 - c) / a0,
            b2: (1.0 - c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
        }
    }

    /// Moves the cutoff of a running low-pass filter, keeping its state (no click).
    pub fn set_lowpass(&mut self, sr: f32, freq: f32, q: f32) {
        let f = Self::lowpass(sr, freq, q);
        (self.b0, self.b1, self.b2, self.a1, self.a2) = (f.b0, f.b1, f.b2, f.a1, f.a2);
    }

    pub fn highpass(sr: f32, freq: f32, q: f32) -> Self {
        let w = 2.0 * PI * freq / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + c) / 2.0 / a0,
            b1: -(1.0 + c) / a0,
            b2: (1.0 + c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
        }
    }

    /// Moves the cutoff of a running high-pass filter, keeping its state (no click).
    pub fn set_highpass(&mut self, sr: f32, freq: f32, q: f32) {
        let f = Self::highpass(sr, freq, q);
        (self.b0, self.b1, self.b2, self.a1, self.a2) = (f.b0, f.b1, f.b2, f.a1, f.a2);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Fixed-capacity ring buffer of samples that can be read out in chronological order.
#[derive(Clone, Debug)]
pub struct History {
    buf: Vec<f32>,
    pos: usize,
}

impl History {
    pub fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len],
            pos: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % self.buf.len();
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Copies the most recent `out.len()` samples, oldest first.
    pub fn copy_latest(&self, out: &mut [f32]) {
        let n = out.len().min(self.buf.len());
        let start = (self.pos + self.buf.len() - n) % self.buf.len();
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = self.buf[(start + i) % self.buf.len()];
        }
    }
}
