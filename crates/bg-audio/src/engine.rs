//! Device management and stream construction.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use bg_sound::{SoundRole, SoundSource, Voice};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, ErrorKind, SampleFormat, Stream, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::clock::Clock;
use crate::config::{AnalysisConfig, AudioConfig, SoundConfig};
use crate::output::{ControlMsg, Garbage, OutMsg, OutputState};

type OutputParts = (
    Stream,
    Producer<ControlMsg>,
    Consumer<Garbage>,
    Producer<OutMsg>,
);
use crate::recording::{Recording, SessionId};
use crate::session::{SessionLogic, Slot};
use crate::worker::{
    self, InChunk, InputLink, InputMeter, RecordSpec, TunerState, WorkerCmd, WorkerShared,
};

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub channels: u16,
    pub default_rate: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceLists {
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
}

#[derive(Clone, Debug, Default)]
pub struct EngineStatus {
    pub output: Option<String>,
    pub input: Option<String>,
    pub output_rate: u32,
    pub input_rate: u32,
    pub input_channels: u16,
    pub output_error: Option<String>,
    pub input_error: Option<String>,
    /// Input and output share one device (and therefore one stream configuration).
    pub shared_device: bool,
}

/// Stream health reported from the cpal error callbacks.
#[derive(Default)]
struct Health {
    /// A stream must be rebuilt (device gone, configuration invalidated, ...).
    failed: AtomicBool,
    /// Buffer under/overruns (glitches); the streams keep running.
    xruns: AtomicU32,
}

fn error_handler(
    direction: &'static str,
    health: Arc<Health>,
) -> impl FnMut(cpal::Error) + Send + 'static {
    move |e: cpal::Error| match e.kind() {
        ErrorKind::Xrun => {
            health.xruns.fetch_add(1, Ordering::Relaxed);
        }
        ErrorKind::DeviceNotAvailable
        | ErrorKind::StreamInvalidated
        | ErrorKind::DeviceBusy
        | ErrorKind::HostUnavailable
        | ErrorKind::BackendError => {
            log::error!("{direction} stream failed: {e}");
            health.failed.store(true, Ordering::Release);
        }
        _ => log::warn!("{direction} stream: {e}"),
    }
}

pub struct AudioEngine {
    host: cpal::Host,
    config: AudioConfig,
    sounds: SoundConfig,
    clock: Clock,
    out_stream: Option<Stream>,
    in_stream: Option<Stream>,
    control: Option<Producer<ControlMsg>>,
    garbage: Option<Consumer<Garbage>>,
    worker_tx: Sender<WorkerCmd>,
    worker: Option<JoinHandle<()>>,
    shared: WorkerShared,
    health: Arc<Health>,
    retry_at: Option<Instant>,
    /// Consecutive rebuilds caused by failures; drives the retry back-off.
    failures: u32,
    last_rebuild: Instant,
    /// The open output device and its stream configuration, reused for the input when
    /// both are the same device (CoreAudio glitches when one device runs two configs).
    out_device: Option<(Device, StreamConfig)>,
    status: EngineStatus,
    next_session: u64,
}

impl AudioEngine {
    pub fn new(config: AudioConfig, sounds: SoundConfig, analysis: AnalysisConfig) -> Self {
        let (tx, rx) = channel();
        let clock = Clock::default();
        let shared = WorkerShared::default();
        let worker = {
            let clock = clock.clone();
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("bg-analysis".into())
                .spawn(move || worker::run(rx, clock, shared))
                .ok()
        };
        let _ = tx.send(WorkerCmd::SetAnalysis(analysis));
        let mut engine = Self {
            host: cpal::default_host(),
            config,
            sounds,
            clock,
            out_stream: None,
            in_stream: None,
            control: None,
            garbage: None,
            worker_tx: tx,
            worker,
            shared,
            health: Arc::default(),
            retry_at: None,
            failures: 0,
            last_rebuild: Instant::now(),
            out_device: None,
            status: EngineStatus::default(),
            next_session: 0,
        };
        engine.rebuild();
        engine
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    pub fn status(&self) -> &EngineStatus {
        &self.status
    }

    /// Number of buffer under/overruns since the streams were opened.
    pub fn xruns(&self) -> u32 {
        self.health.xruns.load(Ordering::Relaxed)
    }

    pub fn config(&self) -> &AudioConfig {
        &self.config
    }

    pub fn tuner(&self) -> TunerState {
        self.shared.tuner.lock().map(|t| *t).unwrap_or_default()
    }

    pub fn meter(&self) -> InputMeter {
        self.shared.meter.lock().map(|m| *m).unwrap_or_default()
    }

    pub fn session_running(&self) -> bool {
        self.shared.session_running.load(Ordering::Acquire)
    }

    pub fn start_session(&self, logic: Box<dyn SessionLogic>) {
        self.start(logic, None);
    }

    /// Starts a session that records its input (keeping at most `max_bytes` of the latest
    /// audio) and scheduled sounds. Once it has ended, [`take_recording`](Self::take_recording)
    /// returns the result.
    pub fn start_recorded(&mut self, logic: Box<dyn SessionLogic>, max_bytes: usize) -> SessionId {
        self.next_session += 1;
        let id = SessionId(self.next_session);
        self.start(logic, Some(RecordSpec { id, max_bytes }));
        id
    }

    fn start(&self, logic: Box<dyn SessionLogic>, spec: Option<RecordSpec>) {
        // Set eagerly so the UI doesn't see a stale "stopped" state for a frame.
        self.shared.session_running.store(true, Ordering::Release);
        let _ = self.worker_tx.send(WorkerCmd::Start(logic, spec));
    }

    /// The recording of session `id`, once that session has ended. Handed over only once.
    pub fn take_recording(&self, id: SessionId) -> Option<Arc<Recording>> {
        let mut slot = self.shared.recording.lock().ok()?;
        if slot.as_ref().is_some_and(|(s, _)| *s == id) {
            slot.take().map(|(_, r)| r)
        } else {
            None
        }
    }

    pub fn stop_session(&self) {
        let _ = self.worker_tx.send(WorkerCmd::Stop);
    }

    pub fn set_analysis(&self, cfg: AnalysisConfig) {
        let _ = self.worker_tx.send(WorkerCmd::SetAnalysis(cfg));
    }

    pub fn set_config(&mut self, config: AudioConfig) {
        if config != self.config {
            self.config = config;
            self.rebuild();
        }
    }

    pub fn set_sounds(&mut self, sounds: SoundConfig) {
        if sounds == self.sounds {
            return;
        }
        let changed = |a: &str, b: &str| a != b;
        let old = std::mem::replace(&mut self.sounds, sounds);
        let sr = self.status.output_rate as f32;
        let new = &self.sounds;
        if let Some(ctl) = self.control.as_mut() {
            for (slot, o, n) in [
                (Slot::Click, &old.click, &new.click),
                (Slot::Groove, &old.groove, &new.groove),
                (Slot::Instrument, &old.instrument, &new.instrument),
            ] {
                if changed(o, n) {
                    let _ = ctl.push(ControlMsg::Voice(slot, make_voice(n, slot, sr)));
                }
            }
            if old.master != new.master {
                let _ = ctl.push(ControlMsg::Master(new.master));
            }
            if (old.monitor, old.monitor_gain) != (new.monitor, new.monitor_gain) {
                let _ = ctl.push(ControlMsg::Monitor {
                    enabled: new.monitor,
                    gain: new.monitor_gain,
                });
            }
            if old.monitor_lowcut() != new.monitor_lowcut() {
                let _ = ctl.push(ControlMsg::MonitorLowcut(new.monitor_lowcut()));
            }
            if old.lowpass() != new.lowpass() {
                let _ = ctl.push(ControlMsg::InstrumentLowpass(new.lowpass()));
            }
        }
    }

    /// Call regularly (e.g. every UI frame): frees retired voices and recovers from
    /// stream failures such as an unplugged interface.
    pub fn poll(&mut self) {
        if let Some(g) = self.garbage.as_mut() {
            while g.pop().is_ok() {}
        }
        if self.health.failed.swap(false, Ordering::AcqRel) && self.retry_at.is_none() {
            // Back off when rebuilding keeps failing, instead of hammering the device.
            if self.last_rebuild.elapsed() > Duration::from_secs(10) {
                self.failures = 0;
            }
            let delay = Duration::from_secs(1 << self.failures.min(4));
            self.failures += 1;
            log::warn!("reopening audio in {}s", delay.as_secs());
            self.retry_at = Some(Instant::now() + delay);
        }
        if self.retry_at.is_some_and(|t| Instant::now() >= t) {
            self.retry_at = None;
            self.rebuild();
        }
    }

    pub fn devices(&self) -> DeviceLists {
        let info = |d: &Device, input: bool| -> Option<DeviceInfo> {
            let cfg = if input {
                d.default_input_config()
            } else {
                d.default_output_config()
            }
            .ok()?;
            Some(DeviceInfo {
                id: d.id().ok()?.to_string(),
                name: device_name(d),
                channels: cfg.channels(),
                default_rate: cfg.sample_rate(),
            })
        };
        DeviceLists {
            inputs: self
                .host
                .input_devices()
                .map(|it| it.filter_map(|d| info(&d, true)).collect())
                .unwrap_or_default(),
            outputs: self
                .host
                .output_devices()
                .map(|it| it.filter_map(|d| info(&d, false)).collect())
                .unwrap_or_default(),
        }
    }

    /// (Re)opens the configured devices.
    pub fn rebuild(&mut self) {
        let _ = self.worker_tx.send(WorkerCmd::Stop);
        let _ = self.worker_tx.send(WorkerCmd::SetOutput(None));
        let _ = self.worker_tx.send(WorkerCmd::SetInput(None));
        self.out_stream = None;
        self.in_stream = None;
        self.control = None;
        self.out_device = None;
        self.clock.set_stopped();
        self.status = EngineStatus::default();
        self.health.failed.store(false, Ordering::Release);
        self.health.xruns.store(0, Ordering::Relaxed);
        self.last_rebuild = Instant::now();

        match self.build_output() {
            Ok(()) => {}
            Err(e) => {
                log::error!("output: {e:#}");
                self.status.output_error = Some(format!("{e:#}"));
            }
        }
        match self.build_input() {
            Ok(()) => {}
            Err(e) => {
                log::error!("input: {e:#}");
                self.status.input_error = Some(format!("{e:#}"));
            }
        }
    }

    fn find_device(&self, id: &Option<String>, input: bool) -> Result<Device> {
        if let Some(id) = id {
            let parsed = id
                .parse()
                .map_err(|e| anyhow!("bad device id {id}: {e:?}"))?;
            if let Some(d) = self.host.device_by_id(&parsed) {
                return Ok(d);
            }
            log::warn!("device {id} not found; using the default");
        }
        if input {
            self.host.default_input_device()
        } else {
            self.host.default_output_device()
        }
        .context("no default device")
    }

    fn stream_config(&self, default: cpal::SupportedStreamConfig) -> StreamConfig {
        let mut cfg = default.config();
        if let Some(sr) = self.config.sample_rate {
            cfg.sample_rate = sr;
        }
        if let Some(frames) = self.config.buffer_frames {
            cfg.buffer_size = BufferSize::Fixed(frames);
        }
        cfg
    }

    fn build_output(&mut self) -> Result<()> {
        let device = self.find_device(&self.config.output_device, false)?;
        let default = device.default_output_config()?;
        let format = default.sample_format();
        let custom = self.stream_config(default);
        let ((stream, ctl, garbage, events), config) =
            match self.try_output(&device, format, &custom) {
                Ok(parts) => (parts, custom),
                // Custom rates / buffer sizes are not supported everywhere; fall back to defaults.
                Err(e) if custom != default.config() => {
                    log::warn!("output with custom settings failed ({e:#}); using device defaults");
                    (
                        self.try_output(&device, format, &default.config())?,
                        default.config(),
                    )
                }
                Err(e) => return Err(e),
            };
        let sr = config.sample_rate;
        stream.play()?;
        self.out_stream = Some(stream);
        self.control = Some(ctl);
        self.garbage = Some(garbage);
        let _ = self.worker_tx.send(WorkerCmd::SetOutput(Some(events)));
        self.status.output = Some(device_name(&device));
        self.status.output_rate = sr;
        self.out_device = Some((device, config));
        Ok(())
    }

    fn try_output(
        &self,
        device: &Device,
        format: SampleFormat,
        config: &StreamConfig,
    ) -> Result<OutputParts> {
        let sr = config.sample_rate;
        let (ev_tx, ev_rx) = RingBuffer::new(4096);
        let (ctl_tx, ctl_rx) = RingBuffer::new(64);
        let (garbage_tx, garbage_rx) = RingBuffer::new(64);
        let voices = [
            Some(make_voice(&self.sounds.click, Slot::Click, sr as f32)),
            Some(make_voice(&self.sounds.groove, Slot::Groove, sr as f32)),
            Some(make_voice(
                &self.sounds.instrument,
                Slot::Instrument,
                sr as f32,
            )),
        ];
        let mut state = OutputState::new(
            self.clock.clone(),
            sr,
            config.channels as usize,
            ev_rx,
            ctl_rx,
            garbage_tx,
            voices,
            self.sounds.master,
        );
        state.set_monitor(self.sounds.monitor, self.sounds.monitor_gain);
        state.set_monitor_lowcut(self.sounds.monitor_lowcut());
        state.set_lowpass(self.sounds.lowpass());
        let on_err = error_handler("output", self.health.clone());
        let stream = match format {
            SampleFormat::F32 => build_out::<f32>(device, config, state, on_err),
            SampleFormat::I16 => build_out::<i16>(device, config, state, on_err),
            SampleFormat::I32 => build_out::<i32>(device, config, state, on_err),
            other => Err(anyhow!("unsupported output sample format {other:?}")),
        }?;
        Ok((stream, ctl_tx, garbage_rx, ev_tx))
    }

    fn build_input(&mut self) -> Result<()> {
        let device = self.find_device(&self.config.input_device, true)?;
        let default = device.default_input_config()?;
        let format = default.sample_format();
        let mut preferred = self.stream_config(default);
        // Prefer running both streams at the output rate when the input supports it.
        if self.config.sample_rate.is_none() && self.status.output_rate != 0 {
            let rate = self.status.output_rate;
            let supported = device
                .supported_input_configs()
                .map(|mut it| {
                    it.any(|r| r.min_sample_rate() <= rate && rate <= r.max_sample_rate())
                })
                .unwrap_or(false);
            if supported {
                preferred.sample_rate = rate;
            }
        }
        // Some devices refuse rate / buffer changes (e.g. CoreAudio "sample rate update
        // timed out"); fall back to the device defaults. A device that is also the output
        // must run with the output's rate and buffer size.
        let mut candidates = vec![preferred];
        if let Some((out_dev, out_cfg)) = &self.out_device
            && *out_dev == device
        {
            self.status.shared_device = true;
            candidates = vec![StreamConfig {
                channels: preferred.channels,
                ..*out_cfg
            }];
        }
        if preferred != default.config() {
            candidates.push(default.config());
        }
        let mut last_err = None;
        for config in candidates {
            match self.try_input(&device, format, config) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    log::warn!(
                        "input with {} Hz / {:?} failed: {e:#}",
                        config.sample_rate,
                        config.buffer_size
                    );
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("no input configuration")))
    }

    fn try_input(
        &mut self,
        device: &Device,
        format: SampleFormat,
        config: StreamConfig,
    ) -> Result<()> {
        let sr = config.sample_rate;
        let channels = config.channels as usize;
        let channel = self
            .config
            .input_channel
            .map(|c| (c as usize).min(channels - 1));

        let (s_tx, s_rx) = RingBuffer::new(sr as usize * 2);
        let (c_tx, c_rx) = RingBuffer::new(1024);
        let (m_tx, m_rx) = RingBuffer::new(sr as usize / 2);
        let sink = InputSink {
            samples: s_tx,
            chunks: c_tx,
            monitor: m_tx,
            clock: self.clock.clone(),
            channels,
            channel,
            dropped: 0,
        };
        let on_err = error_handler("input", self.health.clone());
        let stream = match format {
            SampleFormat::F32 => build_in::<f32>(device, &config, sink, on_err),
            SampleFormat::I16 => build_in::<i16>(device, &config, sink, on_err),
            SampleFormat::I32 => build_in::<i32>(device, &config, sink, on_err),
            other => Err(anyhow!("unsupported input sample format {other:?}")),
        }?;
        stream.play()?;
        self.in_stream = Some(stream);
        if let Some(ctl) = self.control.as_mut() {
            let _ = ctl.push(ControlMsg::MonitorSource(m_rx, sr));
        }
        let _ = self.worker_tx.send(WorkerCmd::SetInput(Some(InputLink {
            samples: s_rx,
            chunks: c_rx,
            sample_rate: sr as f32,
        })));
        self.status.input = Some(device_name(device));
        self.status.input_rate = sr;
        self.status.input_channels = channels as u16;
        Ok(())
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.out_stream = None;
        self.in_stream = None;
        let _ = self.worker_tx.send(WorkerCmd::Shutdown);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

fn device_name(d: &Device) -> String {
    d.description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| d.to_string())
}

fn make_voice(id: &str, slot: Slot, sr: f32) -> Box<dyn Voice> {
    let role = if slot == Slot::Instrument {
        SoundRole::Pitched
    } else {
        SoundRole::Click
    };
    let src: &dyn SoundSource =
        bg_sound::source(id).unwrap_or_else(|| bg_sound::default_source(role));
    src.create(sr.max(8000.0))
}

fn build_out<T>(
    device: &Device,
    config: &StreamConfig,
    mut state: OutputState,
    on_err: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    Ok(device.build_output_stream::<T, _, _>(
        *config,
        move |data, info| state.process(data, info),
        on_err,
        None,
    )?)
}

struct InputSink {
    samples: Producer<f32>,
    chunks: Producer<InChunk>,
    /// Raw input for monitoring through the output.
    monitor: Producer<f32>,
    clock: Clock,
    channels: usize,
    channel: Option<usize>,
    /// Frames dropped since the last chunk went through.
    dropped: u32,
}

impl InputSink {
    fn process<T>(&mut self, data: &[T], info: &cpal::InputCallbackInfo)
    where
        T: cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        use cpal::Sample;
        let frames = data.len() / self.channels.max(1);
        if self.samples.slots() < frames || self.chunks.slots() == 0 {
            // Worker stalled; drop rather than block.
            self.dropped = self.dropped.saturating_add(frames as u32);
            return;
        }
        let time = self
            .clock
            .map_host_ns(info.timestamp().capture.as_nanos() as u64);
        for frame in data.chunks(self.channels.max(1)) {
            let s = match self.channel {
                Some(c) => f32::from_sample(frame[c]),
                None => {
                    frame.iter().map(|&x| f32::from_sample(x)).sum::<f32>() / frame.len() as f32
                }
            };
            let _ = self.samples.push(s);
            let _ = self.monitor.push(s);
        }
        // After its samples, so the worker never sees a chunk it can't read in full.
        let _ = self.chunks.push(InChunk {
            frames: frames as u32,
            time,
            dropped: std::mem::take(&mut self.dropped),
        });
    }
}

fn build_in<T>(
    device: &Device,
    config: &StreamConfig,
    mut sink: InputSink,
    on_err: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    Ok(device.build_input_stream::<T, _, _>(
        *config,
        move |data, info| sink.process(data, info),
        on_err,
        None,
    )?)
}
