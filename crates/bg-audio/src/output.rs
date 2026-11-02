//! Output callback: a sample-accurate event scheduler feeding the voices.

use bg_dsp::filter::Biquad;
use bg_sound::{SoundEvent, Voice};
use rtrb::{Consumer, Producer};

use crate::clock::{Clock, ClockSnapshot};
use crate::session::Slot;

pub(crate) enum OutMsg {
    Play { at: f64, slot: Slot, ev: SoundEvent },
    StopAll,
}

pub(crate) enum ControlMsg {
    Voice(Slot, Box<dyn Voice>),
    Master(f32),
    Monitor {
        enabled: bool,
        gain: f32,
    },
    /// Low-cut frequency (Hz) on the monitored input; `None` = unfiltered.
    MonitorLowcut(Option<f32>),
    /// Low-pass cutoff (Hz) for the instrument slot; `None` = unfiltered.
    InstrumentLowpass(Option<f32>),
    /// Raw input samples and their sample rate, for monitoring.
    MonitorSource(Consumer<f32>, u32),
}

/// Replaced objects are handed back so they're not deallocated on the audio thread.
#[allow(dead_code)]
pub(crate) enum Garbage {
    Voice(Box<dyn Voice>),
    Monitor(Consumer<f32>),
}

/// Plays the input through the output: resamples between the two stream rates and keeps
/// the queued audio short (dropping samples when the input runs ahead, waiting to refill
/// after running dry), so monitoring adds only ~10 ms on top of the device buffers.
struct Monitor {
    src: Consumer<f32>,
    step: f64,
    pos: f64,
    cur: f32,
    next: f32,
    target: usize,
    max: usize,
    priming: bool,
}

impl Monitor {
    fn new(src: Consumer<f32>, in_rate: u32, out_rate: u32) -> Self {
        let target = (in_rate as usize / 100).max(64);
        Self {
            src,
            step: in_rate as f64 / out_rate.max(1) as f64,
            pos: 0.0,
            cur: 0.0,
            next: 0.0,
            target,
            max: target + in_rate as usize / 25,
            priming: true,
        }
    }

    fn skip(&mut self, n: usize) {
        if let Ok(chunk) = self.src.read_chunk(n.min(self.src.slots())) {
            chunk.commit_all();
        }
    }

    /// Discards everything queued (monitoring off).
    fn drain(&mut self) {
        self.skip(self.src.slots());
        self.priming = true;
    }

    #[inline]
    fn sample(&mut self) -> f32 {
        let avail = self.src.slots();
        if avail > self.max {
            self.skip(avail - self.target);
        }
        if self.priming {
            if avail < self.target {
                return 0.0;
            }
            self.priming = false;
        }
        self.pos += self.step;
        while self.pos >= 1.0 {
            self.pos -= 1.0;
            self.cur = self.next;
            match self.src.pop() {
                Ok(s) => self.next = s,
                Err(_) => {
                    self.priming = true;
                    return self.cur;
                }
            }
        }
        self.cur + (self.next - self.cur) * self.pos as f32
    }
}

/// The quiet-practice low cut: two cascaded high-pass sections (4th-order Butterworth,
/// 24 dB/octave), steep enough to take out the booming fundamentals while the notes'
/// upper harmonics (and so their pitch) stay audible.
#[derive(Default)]
struct LowCut {
    cutoff: Option<f32>,
    filters: [Biquad; 2],
}

impl LowCut {
    /// Engages (`Some(Hz)`), retunes or bypasses (`None`) the filter.
    fn set(&mut self, sample_rate: f32, cutoff: Option<f32>) {
        if let Some(hz) = cutoff {
            let hz = hz.clamp(20.0, sample_rate * 0.45);
            for (f, q) in self.filters.iter_mut().zip([0.541, 1.307]) {
                if self.cutoff.is_some() {
                    // Keep the running state when retuning, so moving the cutoff doesn't click.
                    f.set_highpass(sample_rate, hz, q);
                } else {
                    *f = Biquad::highpass(sample_rate, hz, q);
                }
            }
        }
        self.cutoff = cutoff;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        if self.cutoff.is_some() {
            self.filters.iter_mut().fold(x, |y, f| f.process(y))
        } else {
            x
        }
    }
}

const BLOCK: usize = 16;
const MAX_PENDING: usize = 4096;

pub(crate) struct OutputState {
    pub clock: Clock,
    pub sample_rate: u32,
    pub channels: usize,
    pub events: Consumer<OutMsg>,
    pub control: Consumer<ControlMsg>,
    /// Old voices are handed back so they're not deallocated on the audio thread.
    pub garbage: Producer<Garbage>,
    pub voices: [Option<Box<dyn Voice>>; 3],
    pub master: f32,
    pub frames: u64,
    monitor: Option<Monitor>,
    monitor_on: bool,
    monitor_gain: f32,
    monitor_lowcut: LowCut,
    lowpass: Option<f32>,
    /// Two cascaded sections (24 dB/octave), so the cutoff works as a tone control.
    lowpass_filters: [Biquad; 2],
    pending: Vec<(u64, Slot, SoundEvent)>,
    mix: Vec<f32>,
    inst: Vec<f32>,
}

impl OutputState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        clock: Clock,
        sample_rate: u32,
        channels: usize,
        events: Consumer<OutMsg>,
        control: Consumer<ControlMsg>,
        garbage: Producer<Garbage>,
        voices: [Option<Box<dyn Voice>>; 3],
        master: f32,
    ) -> Self {
        Self {
            clock,
            sample_rate,
            channels,
            events,
            control,
            garbage,
            voices,
            master,
            frames: 0,
            monitor: None,
            monitor_on: false,
            monitor_gain: 0.0,
            monitor_lowcut: LowCut::default(),
            lowpass: None,
            lowpass_filters: Default::default(),
            pending: Vec::with_capacity(MAX_PENDING),
            mix: vec![0.0; BLOCK],
            inst: vec![0.0; BLOCK],
        }
    }

    pub fn set_lowpass(&mut self, cutoff: Option<f32>) {
        if let Some(hz) = cutoff {
            let hz = hz.clamp(100.0, self.sample_rate as f32 * 0.45);
            // Butterworth Q values for a 4th-order response.
            for (f, q) in self.lowpass_filters.iter_mut().zip([0.541, 1.307]) {
                if self.lowpass.is_some() {
                    f.set_lowpass(self.sample_rate as f32, hz, q);
                } else {
                    *f = Biquad::lowpass(self.sample_rate as f32, hz, q);
                }
            }
        }
        self.lowpass = cutoff;
    }

    pub fn set_monitor_lowcut(&mut self, cutoff: Option<f32>) {
        self.monitor_lowcut.set(self.sample_rate as f32, cutoff);
    }

    pub fn set_monitor(&mut self, enabled: bool, gain: f32) {
        self.monitor_on = enabled;
        self.monitor_gain = gain;
    }

    pub fn process<T: cpal::SizedSample + cpal::FromSample<f32>>(
        &mut self,
        data: &mut [T],
        info: &cpal::OutputCallbackInfo,
    ) {
        let ts = info.timestamp();
        self.clock.publish(ClockSnapshot {
            frames: self.frames,
            sample_rate: self.sample_rate,
            playback_ns: ts.playback.as_nanos() as u64,
            callback_ns: ts.callback.as_nanos() as u64,
            local_ns: self.clock.local_ns(),
        });

        while let Ok(msg) = self.control.pop() {
            match msg {
                ControlMsg::Voice(slot, v) => {
                    if let Some(old) = self.voices[slot as usize].replace(v) {
                        let _ = self.garbage.push(Garbage::Voice(old));
                    }
                }
                ControlMsg::Master(m) => self.master = m,
                ControlMsg::Monitor { enabled, gain } => self.set_monitor(enabled, gain),
                ControlMsg::MonitorLowcut(cutoff) => self.set_monitor_lowcut(cutoff),
                ControlMsg::InstrumentLowpass(cutoff) => self.set_lowpass(cutoff),
                ControlMsg::MonitorSource(src, rate) => {
                    if let Some(old) =
                        self.monitor
                            .replace(Monitor::new(src, rate, self.sample_rate))
                    {
                        let _ = self.garbage.push(Garbage::Monitor(old.src));
                    }
                }
            }
        }
        while let Ok(msg) = self.events.pop() {
            match msg {
                OutMsg::Play { at, slot, ev } => {
                    if self.pending.len() < MAX_PENDING {
                        let frame = (at * self.sample_rate as f64).round().max(0.0) as u64;
                        self.pending.push((frame, slot, ev));
                    }
                }
                OutMsg::StopAll => {
                    self.pending.clear();
                    self.voices.iter_mut().flatten().for_each(|v| v.silence());
                }
            }
        }

        let channels = self.channels.max(1);
        for frame_block in data.chunks_mut(BLOCK * channels) {
            let n = frame_block.len() / channels;
            let block_end = self.frames + n as u64;
            // Trigger events due in this block (≤ 16 frames of quantization).
            let mut i = 0;
            while i < self.pending.len() {
                if self.pending[i].0 < block_end {
                    let (_, slot, ev) = self.pending.swap_remove(i);
                    if let Some(v) = self.voices[slot as usize].as_mut() {
                        v.trigger(&ev);
                    }
                } else {
                    i += 1;
                }
            }
            let mix = &mut self.mix[..n];
            mix.fill(0.0);
            let [click, groove, inst] = &mut self.voices;
            for v in [click, groove].into_iter().flatten() {
                v.render(mix);
            }
            if let Some(v) = inst {
                let buf = &mut self.inst[..n];
                buf.fill(0.0);
                v.render(buf);
                if self.lowpass.is_some() {
                    for x in buf.iter_mut() {
                        *x = self
                            .lowpass_filters
                            .iter_mut()
                            .fold(*x, |y, f| f.process(y));
                    }
                }
                mix.iter_mut().zip(buf.iter()).for_each(|(m, x)| *m += x);
            }
            if let Some(m) = self.monitor.as_mut() {
                if self.monitor_on {
                    for x in mix.iter_mut() {
                        *x += self.monitor_lowcut.process(m.sample()) * self.monitor_gain;
                    }
                } else {
                    m.drain();
                }
            }
            for (frame, &m) in frame_block.chunks_mut(channels).zip(mix.iter()) {
                let s = soft_clip(m * self.master);
                frame.fill(T::from_sample(s));
            }
            self.frames = block_end;
        }
    }
}

fn soft_clip(x: f32) -> f32 {
    if x.abs() < 0.8 {
        x
    } else {
        x.signum() * (0.8 + 0.2 * ((x.abs() - 0.8) / 0.2).tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtrb::RingBuffer;

    /// RMS gain (dB) of `f` on a sine at `hz`, after the filter has settled.
    fn gain_db(f: &mut LowCut, hz: f32) -> f32 {
        let sr = 48_000.0;
        let sine = |i: usize| (2.0 * std::f32::consts::PI * hz * i as f32 / sr).sin();
        let out: Vec<f32> = (0..48_000).map(|i| f.process(sine(i))).collect();
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let input: Vec<f32> = (24_000..48_000).map(sine).collect();
        20.0 * (rms(&out[24_000..]) / rms(&input)).log10()
    }

    #[test]
    fn quiet_low_cut_removes_lows_and_keeps_highs() {
        let mut f = LowCut::default();
        assert!(gain_db(&mut f, 50.0).abs() < 0.1, "bypassed by default");
        f.set(48_000.0, Some(150.0));
        // Low E (41 Hz) is cut hard, the cutoff is -3 dB, upper harmonics pass.
        assert!(gain_db(&mut f, 41.0) < -40.0);
        assert!((gain_db(&mut f, 150.0) + 3.0).abs() < 0.5);
        assert!(gain_db(&mut f, 1_000.0).abs() < 0.1);
        f.set(48_000.0, None);
        assert!(gain_db(&mut f, 41.0).abs() < 0.1, "bypassed again");
    }

    #[test]
    fn monitor_resamples_and_bounds_latency() {
        let (mut tx, rx) = RingBuffer::new(48_000);
        let mut m = Monitor::new(rx, 44_100, 48_000);
        // Primes until ~10 ms is queued, then plays a continuous ramp.
        assert_eq!(m.sample(), 0.0);
        for i in 0..2_000 {
            tx.push(i as f32).unwrap();
        }
        let out: Vec<f32> = (0..1_000).map(|_| m.sample()).collect();
        let diffs: Vec<f32> = out.windows(2).skip(2).map(|w| w[1] - w[0]).collect();
        assert!(
            diffs.iter().all(|d| (d - 44_100.0 / 48_000.0).abs() < 1e-3),
            "{:?}",
            &diffs[..8]
        );
        // A large backlog is trimmed back to the target instead of adding latency.
        for i in 0..20_000 {
            tx.push(i as f32).unwrap();
        }
        m.sample();
        assert!(m.src.slots() <= m.target + 1, "{} queued", m.src.slots());
    }
}
