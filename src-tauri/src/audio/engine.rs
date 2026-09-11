//! The audio side: one output stream, the thread that looks after it, and the
//! path a click takes from the render callback to the interface.
//!
//! Three kinds of thread meet here and none of them waits on another:
//!
//! * The render callback, owned by cpal. It reads the tempo and meter from
//!   atomics, writes clicks into the device buffer and drops a note of each
//!   click into a lock free ring. It never locks, allocates or logs.
//! * The control thread. It owns the stream: opens it on play, closes it a few
//!   seconds after stop so a USB interface is not kept awake for nothing, and
//!   falls back to the default output when the chosen one goes away.
//! * The beat thread. It drains the ring and hands each click to the app, which
//!   forwards it to the webview. Nothing inside the callback talks to Tauri.
//!
//! When there is no device at all, or the app runs with `--demo`, a silent
//! driver runs the same clock against the wall clock so the widget still keeps
//! time on screen.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;

use super::clock::{Clock, Kind, Params, Tick};
use super::voice::{self, Bank, Sound};
use crate::settings::MetronomeSettings;

/// How long an idle stream stays open after stop. Long enough that stop and
/// play in quick succession never reopen the device, short enough that an
/// interface is not held for a metronome nobody is using.
const CLOSE_AFTER: Duration = Duration::from_secs(3);

/// How often a missing device is looked for again while playing.
const RETRY_EVERY: Duration = Duration::from_secs(2);

/// Enumerating endpoints is slow, so the list is refreshed on its own thread
/// on this cadence rather than on every tick of anything.
const SCAN_EVERY: Duration = Duration::from_secs(10);

/// Sounds that can ring at once. Sixteenths at 300 bpm are 50 ms apart; a
/// click is gone before the next one and an open hi-hat lasts a few of them,
/// so sixteen is room with plenty to spare.
const MAX_VOICES: usize = 16;

/// Sample rate of the silent driver.
const SILENT_RATE: u32 = 48_000;

/// A click as the interface hears about it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BeatEvent {
    pub bar: u32,
    pub beat: u8,
    pub sub: u8,
    /// "accent", "beat" or "sub".
    pub kind: &'static str,
    pub beats_per_bar: u8,
    pub subdivision: u8,
    pub bpm: u32,
    /// When the click leaves the speaker, in milliseconds since the Unix
    /// epoch: the callback's wall time plus the output latency the device
    /// reports plus the click's offset into the block. The interface lines its
    /// animation up with this rather than with the moment the event arrives.
    pub at_ms: f64,
}

/// What the tray and the diagnostics report need to know about the output.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Status {
    /// The device the stream is open on, if one is.
    pub active_device: Option<String>,
    /// A problem worth telling the user about, in words.
    pub problem: Option<String>,
    /// Every output endpoint, by name, as of the last scan.
    pub devices: Vec<String>,
    pub sample_rate: Option<u32>,
    /// The clock is running without a device.
    pub silent: bool,
}

/// Counters the render callback keeps for the diagnostic report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StatsSnapshot {
    pub callbacks: u64,
    pub frames: u64,
    pub max_callback_us: u64,
    pub xruns: u64,
    pub output_latency_us: u64,
    /// How far the device clock and the system clock disagree, in parts per
    /// million, measured over the life of the current stream. It says nothing
    /// about the click grid, which is exact by construction; it says whether
    /// the device plays at the rate it claims.
    pub device_clock_ppm: Option<f64>,
}

pub enum Notice {
    Beat(BeatEvent),
    Status,
}

pub type Notify = Arc<dyn Fn(Notice) + Send + Sync>;

enum Cmd {
    Start,
    Stop,
    SetDevice(Option<String>),
    StreamFailed(String),
    DevicesChanged,
    Shutdown,
}

#[derive(Default)]
struct Stats {
    callbacks: AtomicU64,
    frames: AtomicU64,
    max_callback_ns: AtomicU64,
    xruns: AtomicU64,
    latency_ns: AtomicU64,
    first_playback_ns: AtomicU64,
    frames_at_first: AtomicU64,
    last_playback_ns: AtomicU64,
    frames_at_last: AtomicU64,
    rate: AtomicU32,
}

/// Callbacks to let pass before the clock comparison starts. The first ones
/// fill the device buffer ahead of playback, and measuring from them reads that
/// prefill as the device running fast.
const WARMUP_CALLBACKS: u64 = 50;

impl Stats {
    fn reset(&self, rate: u32) {
        for a in [
            &self.callbacks,
            &self.frames,
            &self.first_playback_ns,
            &self.frames_at_first,
            &self.last_playback_ns,
            &self.frames_at_last,
        ] {
            a.store(0, Ordering::Relaxed);
        }
        self.rate.store(rate, Ordering::Relaxed);
    }
}

/// Everything the render callback reads, as atomics.
struct Shared {
    running: AtomicBool,
    /// Bumped on every start. The callback restarts the pattern when it sees
    /// a new value, which is how play always opens on a downbeat.
    generation: AtomicU32,
    bpm: AtomicU32,
    beats_per_bar: AtomicU8,
    subdivision: AtomicU8,
    accent: AtomicBool,
    sound: AtomicU8,
    volume: AtomicU8,
    ring: BeatRing,
    stats: Stats,
    shutdown: AtomicBool,
}

impl Shared {
    fn params(&self) -> Params {
        Params {
            bpm: self.bpm.load(Ordering::Relaxed),
            beats_per_bar: self.beats_per_bar.load(Ordering::Relaxed),
            subdivision: self.subdivision.load(Ordering::Relaxed),
            accent_first: self.accent.load(Ordering::Relaxed),
        }
    }

    fn store(&self, m: &MetronomeSettings) {
        self.bpm.store(m.bpm, Ordering::Relaxed);
        self.beats_per_bar.store(m.beats_per_bar, Ordering::Relaxed);
        self.subdivision.store(m.subdivision, Ordering::Relaxed);
        self.accent.store(m.accent_first, Ordering::Relaxed);
        self.sound
            .store(Sound::from_name(&m.sound).index(), Ordering::Relaxed);
        self.volume.store(m.volume, Ordering::Relaxed);
    }
}

pub struct Engine {
    shared: Arc<Shared>,
    tx: Mutex<mpsc::Sender<Cmd>>,
    status: Arc<RwLock<Status>>,
    wanted_device: Mutex<Option<String>>,
    beat_thread: std::thread::Thread,
}

impl Engine {
    /// `silent` runs the clock without opening any device, for `--demo`.
    pub fn new(settings: &MetronomeSettings, silent: bool, notify: Notify) -> Arc<Self> {
        let shared = Arc::new(Shared {
            running: AtomicBool::new(false),
            generation: AtomicU32::new(0),
            bpm: AtomicU32::new(settings.bpm),
            beats_per_bar: AtomicU8::new(settings.beats_per_bar),
            subdivision: AtomicU8::new(settings.subdivision),
            accent: AtomicBool::new(settings.accent_first),
            sound: AtomicU8::new(Sound::from_name(&settings.sound).index()),
            volume: AtomicU8::new(settings.volume),
            ring: BeatRing::new(),
            stats: Stats::default(),
            shutdown: AtomicBool::new(false),
        });
        let status = Arc::new(RwLock::new(Status::default()));
        let (tx, rx) = mpsc::channel();

        let beat_thread = {
            let shared = shared.clone();
            let notify = notify.clone();
            std::thread::Builder::new()
                .name("taktdock-beats".into())
                .spawn(move || beat_loop(shared, notify))
                .expect("beat thread")
                .thread()
                .clone()
        };

        if !silent {
            let status = status.clone();
            let notify = notify.clone();
            let tx = tx.clone();
            let shared = shared.clone();
            let _ = std::thread::Builder::new()
                .name("taktdock-devices".into())
                .spawn(move || scan_loop(shared, status, notify, tx));
        }

        {
            let control = Control {
                shared: shared.clone(),
                status: status.clone(),
                notify,
                tx: tx.clone(),
                wanted: settings.output_device.clone(),
                driver: None,
                silent,
            };
            let _ = std::thread::Builder::new()
                .name("taktdock-audio".into())
                .spawn(move || control.run(rx));
        }

        Arc::new(Self {
            shared,
            tx: Mutex::new(tx),
            status,
            wanted_device: Mutex::new(settings.output_device.clone()),
            beat_thread,
        })
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::SeqCst)
    }

    pub fn start(&self) {
        self.send(Cmd::Start);
        self.beat_thread.unpark();
    }

    pub fn stop(&self) {
        // Cleared here as well as on the control thread, so the callback stops
        // scheduling clicks the moment stop is pressed rather than when the
        // command is picked up.
        self.shared.running.store(false, Ordering::SeqCst);
        self.send(Cmd::Stop);
    }

    /// Returns whether it is running afterwards.
    pub fn toggle(&self) -> bool {
        if self.is_running() {
            self.stop();
            false
        } else {
            self.start();
            true
        }
    }

    /// Copies the settings to the audio thread. Cheap enough to call on every
    /// change; only a different output device costs anything.
    pub fn apply(&self, m: &MetronomeSettings) {
        self.shared.store(m);
        let mut wanted = self.wanted_device.lock();
        if *wanted != m.output_device {
            *wanted = m.output_device.clone();
            self.send(Cmd::SetDevice(m.output_device.clone()));
        }
    }

    pub fn status(&self) -> Status {
        self.status.read().clone()
    }

    pub fn stats(&self) -> StatsSnapshot {
        let s = &self.shared.stats;
        let rate = s.rate.load(Ordering::Relaxed) as f64;
        let first = s.first_playback_ns.load(Ordering::Relaxed);
        let last = s.last_playback_ns.load(Ordering::Relaxed);
        let played_frames = s
            .frames_at_last
            .load(Ordering::Relaxed)
            .saturating_sub(s.frames_at_first.load(Ordering::Relaxed));
        let device_clock_ppm =
            if rate > 0.0 && first > 0 && last > first && played_frames > rate as u64 {
                let wall = (last - first) as f64 / 1e9;
                let played = played_frames as f64 / rate;
                Some((wall - played) / played * 1e6)
            } else {
                None
            };
        StatsSnapshot {
            callbacks: s.callbacks.load(Ordering::Relaxed),
            frames: s.frames.load(Ordering::Relaxed),
            max_callback_us: s.max_callback_ns.load(Ordering::Relaxed) / 1000,
            xruns: s.xruns.load(Ordering::Relaxed),
            output_latency_us: s.latency_ns.load(Ordering::Relaxed) / 1000,
            device_clock_ppm,
        }
    }

    pub fn shutdown(&self) {
        self.shared.running.store(false, Ordering::SeqCst);
        self.shared.shutdown.store(true, Ordering::SeqCst);
        self.send(Cmd::Shutdown);
        self.beat_thread.unpark();
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.tx.lock().send(cmd);
    }
}

/// Every output endpoint by name, in the order the system lists them.
pub fn output_device_names() -> Vec<String> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for d in devices {
            if let Ok(desc) = d.description() {
                let name = desc.name().to_string();
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

fn find_device(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    host.output_devices().ok()?.find(|d| {
        d.description()
            .map(|desc| desc.name() == name)
            .unwrap_or(false)
    })
}

fn device_name(d: &cpal::Device) -> String {
    d.description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| "unknown device".into())
}

enum Driver {
    Device {
        _stream: cpal::Stream,
        /// Running on the default because the chosen device is unavailable.
        fallback: bool,
    },
    /// Held for its `Drop`, which stops and joins the thread.
    Silent { _driver: SilentDriver },
}

struct Control {
    shared: Arc<Shared>,
    status: Arc<RwLock<Status>>,
    notify: Notify,
    tx: mpsc::Sender<Cmd>,
    wanted: Option<String>,
    driver: Option<Driver>,
    silent: bool,
}

impl Control {
    fn run(mut self, rx: mpsc::Receiver<Cmd>) {
        let mut close_at: Option<Instant> = None;
        let mut retry_at: Option<Instant> = None;

        loop {
            let now = Instant::now();
            let wake = [close_at, retry_at]
                .into_iter()
                .flatten()
                .min()
                .map(|t| t.saturating_duration_since(now))
                .unwrap_or(Duration::from_secs(3600));

            match rx.recv_timeout(wake) {
                Ok(Cmd::Start) => {
                    close_at = None;
                    if self.driver.is_none() {
                        self.connect();
                    }
                    self.shared.generation.fetch_add(1, Ordering::SeqCst);
                    self.shared.running.store(true, Ordering::SeqCst);
                    retry_at = self.fallback().then(|| Instant::now() + RETRY_EVERY);
                }
                Ok(Cmd::Stop) => {
                    self.shared.running.store(false, Ordering::SeqCst);
                    close_at = Some(Instant::now() + CLOSE_AFTER);
                    retry_at = None;
                }
                Ok(Cmd::SetDevice(name)) => {
                    self.wanted = name;
                    if self.driver.is_some() {
                        self.reconnect();
                    } else {
                        self.clear_problem();
                    }
                    retry_at =
                        (self.running() && self.fallback()).then(|| Instant::now() + RETRY_EVERY);
                }
                Ok(Cmd::StreamFailed(why)) => {
                    tracing::warn!("output stream failed: {why}");
                    self.driver = None;
                    if self.running() {
                        self.connect();
                        self.shared.generation.fetch_add(1, Ordering::SeqCst);
                        retry_at = self.fallback().then(|| Instant::now() + RETRY_EVERY);
                    } else {
                        self.set_status(|s| {
                            s.active_device = None;
                            s.sample_rate = None;
                        });
                    }
                }
                Ok(Cmd::DevicesChanged) => {
                    // A chosen device that has just come back is taken up at
                    // once rather than at the next two second retry.
                    if self.running() && self.fallback() {
                        retry_at = Some(Instant::now());
                    }
                }
                Ok(Cmd::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }

            let now = Instant::now();
            if close_at.is_some_and(|t| now >= t) {
                close_at = None;
                if !self.running() && self.driver.is_some() {
                    tracing::debug!("closing the idle output stream");
                    self.driver = None;
                    self.set_status(|s| {
                        s.active_device = None;
                        s.sample_rate = None;
                        s.silent = false;
                    });
                }
            }
            if retry_at.is_some_and(|t| now >= t) {
                retry_at = None;
                if self.running() && self.fallback() && self.try_preferred() {
                    tracing::info!("the chosen output is available again, switched to it");
                }
                if self.running() && self.fallback() {
                    retry_at = Some(Instant::now() + RETRY_EVERY);
                }
            }
        }
        self.driver = None;
    }

    fn running(&self) -> bool {
        self.shared.running.load(Ordering::SeqCst)
    }

    /// Playing somewhere other than where the user asked for.
    fn fallback(&self) -> bool {
        match &self.driver {
            Some(Driver::Device { fallback, .. }) => *fallback,
            Some(Driver::Silent { .. }) => !self.silent,
            None => false,
        }
    }

    /// Tries the output the user asked for without touching the one that is
    /// playing, and switches only if it opens. Retrying by tearing down the
    /// fallback first would put a gap in the click every two seconds for as
    /// long as, say, a DAW holds the interface.
    fn try_preferred(&mut self) -> bool {
        let host = cpal::default_host();
        let device = match &self.wanted {
            Some(name) => find_device(&host, name),
            None => host.default_output_device(),
        };
        let Some(device) = device else {
            return false;
        };
        let name = device_name(&device);
        let Ok((stream, rate)) = self.open(&device) else {
            return false;
        };
        // The old driver goes before the new stream plays: both would
        // otherwise write to the beat ring, which has room for one writer.
        self.driver = None;
        if let Err(e) = stream.play() {
            tracing::warn!("{name} opened but would not start: {e}");
            self.connect();
            return false;
        }
        self.driver = Some(Driver::Device {
            _stream: stream,
            fallback: false,
        });
        self.set_status(|s| {
            s.active_device = Some(name);
            s.sample_rate = Some(rate);
            s.silent = false;
            s.problem = None;
        });
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn reconnect(&mut self) {
        self.driver = None;
        self.connect();
        if self.running() {
            self.shared.generation.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn clear_problem(&self) {
        self.set_status(|s| s.problem = None);
    }

    fn set_status(&self, f: impl FnOnce(&mut Status)) {
        let changed = {
            let mut s = self.status.write();
            let before = s.clone();
            f(&mut s);
            *s != before
        };
        if changed {
            (self.notify)(Notice::Status);
        }
    }

    /// Opens the chosen device, or the default when it cannot, or the silent
    /// driver when there is nothing at all.
    fn connect(&mut self) {
        self.driver = None;
        if self.silent {
            self.driver = Some(Driver::Silent {
                _driver: SilentDriver::start(self.shared.clone()),
            });
            self.set_status(|s| {
                s.active_device = None;
                s.sample_rate = Some(SILENT_RATE);
                s.silent = true;
                s.problem = None;
            });
            return;
        }

        let host = cpal::default_host();
        let mut problem = None;

        if let Some(name) = self.wanted.clone() {
            match find_device(&host, &name) {
                Some(device) => match self.open(&device).and_then(started) {
                    Ok((stream, rate)) => {
                        tracing::info!("output open on {name} at {rate} Hz");
                        self.driver = Some(Driver::Device {
                            _stream: stream,
                            fallback: false,
                        });
                        self.set_status(|s| {
                            s.active_device = Some(name);
                            s.sample_rate = Some(rate);
                            s.silent = false;
                            s.problem = None;
                        });
                        return;
                    }
                    Err(e) => {
                        tracing::warn!("{name} could not be opened: {e}");
                        problem = Some(format!(
                            "{name} is busy or unavailable, using the default output"
                        ));
                    }
                },
                None => {
                    tracing::warn!("{name} is not connected, using the default output");
                    problem = Some(format!("{name} not found, using the default output"));
                }
            }
        }

        let fallback = self.wanted.is_some();
        match host.default_output_device() {
            Some(device) => {
                let name = device_name(&device);
                match self.open(&device).and_then(started) {
                    Ok((stream, rate)) => {
                        tracing::info!("output open on {name} (default) at {rate} Hz");
                        self.driver = Some(Driver::Device {
                            _stream: stream,
                            fallback,
                        });
                        self.set_status(|s| {
                            s.active_device = Some(name);
                            s.sample_rate = Some(rate);
                            s.silent = false;
                            s.problem = problem;
                        });
                        return;
                    }
                    Err(e) => {
                        tracing::warn!("the default output could not be opened: {e}");
                        problem = Some(format!("No output could be opened ({e})"));
                    }
                }
            }
            None => {
                tracing::warn!("no output device at all");
                problem = Some("No output device".into());
            }
        }

        // Nothing to play on. The widget still keeps time so the pattern is
        // visible, and the tray says why there is no sound.
        self.driver = Some(Driver::Silent {
            _driver: SilentDriver::start(self.shared.clone()),
        });
        self.set_status(|s| {
            s.active_device = None;
            s.sample_rate = Some(SILENT_RATE);
            s.silent = true;
            s.problem = problem;
        });
    }

    /// Builds a stream on this device, paused. Streams come back paused on
    /// every backend, and starting one is left to the caller so it can make
    /// sure no other stream is still writing clicks.
    fn open(&self, device: &cpal::Device) -> Result<(cpal::Stream, u32), String> {
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        // The device's own rate and format. Resampling a click buys nothing.
        let config: cpal::StreamConfig = supported.into();
        let rate = config.sample_rate;

        use cpal::SampleFormat as F;
        let stream = match format {
            F::F32 => self.build::<f32>(device, config),
            F::F64 => self.build::<f64>(device, config),
            F::I16 => self.build::<i16>(device, config),
            F::I32 => self.build::<i32>(device, config),
            F::U16 => self.build::<u16>(device, config),
            F::U8 => self.build::<u8>(device, config),
            F::I8 => self.build::<i8>(device, config),
            other => return Err(format!("unsupported sample format {other}")),
        }?;
        Ok((stream, rate))
    }

    fn build<T>(
        &self,
        device: &cpal::Device,
        config: cpal::StreamConfig,
    ) -> Result<cpal::Stream, String>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        let channels = config.channels.max(1) as usize;
        let rate = config.sample_rate;
        let mut renderer = Renderer::new(rate, self.shared.clone());
        self.shared.stats.reset(rate);
        let shared = self.shared.clone();

        let data = move |out: &mut [T], info: &cpal::OutputCallbackInfo| {
            let began = Instant::now();
            let frames = out.len() / channels;

            let ts = info.timestamp();
            let latency = ts.playback.duration_since(ts.callback);
            let stats = &shared.stats;
            let playback_ns = ts
                .playback
                .duration_since(cpal::StreamInstant::ZERO)
                .as_nanos() as u64;
            let frames_so_far = stats.frames.load(Ordering::Relaxed);
            if stats.callbacks.load(Ordering::Relaxed) == WARMUP_CALLBACKS {
                stats
                    .first_playback_ns
                    .store(playback_ns.max(1), Ordering::Relaxed);
                stats
                    .frames_at_first
                    .store(frames_so_far, Ordering::Relaxed);
            }
            stats.last_playback_ns.store(playback_ns, Ordering::Relaxed);
            stats.frames_at_last.store(frames_so_far, Ordering::Relaxed);
            stats
                .latency_ns
                .store(latency.as_nanos() as u64, Ordering::Relaxed);

            renderer.process(
                frames,
                wall_ms() + latency.as_secs_f64() * 1000.0,
                |frame, v| {
                    let s = T::from_sample(v);
                    let base = frame * channels;
                    for slot in &mut out[base..base + channels] {
                        *slot = s;
                    }
                },
            );

            stats.callbacks.fetch_add(1, Ordering::Relaxed);
            stats.frames.fetch_add(frames as u64, Ordering::Relaxed);
            let took = began.elapsed().as_nanos() as u64;
            stats.max_callback_ns.fetch_max(took, Ordering::Relaxed);
        };

        let tx = self.tx.clone();
        let shared = self.shared.clone();
        let error = move |err: cpal::Error| match err.kind() {
            cpal::ErrorKind::Xrun => {
                shared.stats.xruns.fetch_add(1, Ordering::Relaxed);
            }
            // The stream still runs, just without the MMCSS boost.
            cpal::ErrorKind::RealtimeDenied => {}
            _ => {
                let _ = tx.send(Cmd::StreamFailed(err.to_string()));
            }
        };

        device
            .build_output_stream::<T, _, _>(config, data, error, None)
            .map_err(|e| e.to_string())
    }
}

fn started((stream, rate): (cpal::Stream, u32)) -> Result<(cpal::Stream, u32), String> {
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate))
}

fn wall_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

#[derive(Clone, Copy)]
struct Voice {
    bank: u8,
    kind: Kind,
    /// Frames into the buffer. Negative while the click is still waiting for
    /// its offset in the current block.
    pos: isize,
    active: bool,
}

const IDLE: Voice = Voice {
    bank: 0,
    kind: Kind::Beat,
    pos: 0,
    active: false,
};

/// Clock plus voices: everything the render callback does, shared with the
/// silent driver so both keep the same time.
struct Renderer {
    clock: Clock,
    banks: [Bank; Sound::ALL.len()],
    voices: [Voice; MAX_VOICES],
    seen_generation: u32,
    shared: Arc<Shared>,
}

impl Renderer {
    fn new(rate: u32, shared: Arc<Shared>) -> Self {
        Self {
            clock: Clock::new(rate),
            banks: Sound::ALL.map(|s| Bank::render(s, rate)),
            voices: [IDLE; MAX_VOICES],
            // Whatever start came before this stream belongs to the last one.
            seen_generation: shared.generation.load(Ordering::SeqCst).wrapping_sub(1),
            shared,
        }
    }

    /// Renders one block. `playback_ms` is the wall time the first frame of
    /// the block will be heard; `write` receives every frame's mixed sample.
    fn process(&mut self, frames: usize, playback_ms: f64, mut write: impl FnMut(usize, f32)) {
        let shared = &self.shared;
        let generation = shared.generation.load(Ordering::SeqCst);
        let params = shared.params();
        if generation != self.seen_generation {
            self.seen_generation = generation;
            self.clock.start(params);
        }

        if shared.running.load(Ordering::SeqCst) {
            let bank = shared.sound.load(Ordering::Relaxed);
            let rate = self.clock.rate() as f64;
            let voices = &mut self.voices;
            let ring = &shared.ring;
            self.clock.advance(frames, params, |t: Tick| {
                start_voice(voices, bank, t.kind, t.offset);
                ring.push(&BeatEvent {
                    bar: t.bar,
                    beat: t.beat,
                    sub: t.sub,
                    kind: match t.kind {
                        Kind::Accent => "accent",
                        Kind::Beat => "beat",
                        Kind::Sub => "sub",
                    },
                    beats_per_bar: t.beats_per_bar,
                    subdivision: t.subdivision,
                    bpm: t.bpm,
                    at_ms: playback_ms + t.offset as f64 * 1000.0 / rate,
                });
            });
        }

        let gain = voice::gain(shared.volume.load(Ordering::Relaxed));
        for frame in 0..frames {
            let mut v = 0.0f32;
            for voice in self.voices.iter_mut().filter(|v| v.active) {
                if voice.pos >= 0 {
                    let buf = self.banks[voice.bank as usize].get(voice.kind);
                    match buf.get(voice.pos as usize) {
                        Some(s) => v += *s,
                        None => {
                            voice.active = false;
                            continue;
                        }
                    }
                }
                voice.pos += 1;
            }
            write(frame, voice::soft_limit(v * gain));
        }
    }
}

fn start_voice(voices: &mut [Voice; MAX_VOICES], bank: u8, kind: Kind, offset: usize) {
    // A free slot, or else the one that has been playing longest.
    let slot = voices.iter().position(|v| !v.active).unwrap_or_else(|| {
        voices
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| v.pos)
            .map(|(i, _)| i)
            .unwrap_or(0)
    });
    voices[slot] = Voice {
        bank: bank.min(Sound::ALL.len() as u8 - 1),
        kind,
        pos: -(offset as isize),
        active: true,
    };
}

/// Keeps time against the wall clock when there is no device to keep it.
struct SilentDriver {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SilentDriver {
    fn start(shared: Arc<Shared>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("taktdock-silent".into())
            .spawn(move || {
                let mut renderer = Renderer::new(SILENT_RATE, shared);
                let began = Instant::now();
                let mut done: u64 = 0;
                while !flag.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                    let due = (began.elapsed().as_secs_f64() * SILENT_RATE as f64) as u64;
                    let frames = (due - done) as usize;
                    done = due;
                    // The block is rendered after the fact, so its first frame
                    // happened `frames` ago rather than in the future.
                    let start_ms = wall_ms() - frames as f64 * 1000.0 / SILENT_RATE as f64;
                    renderer.process(frames, start_ms, |_, _| {});
                }
            })
            .ok();
        Self { stop, thread }
    }
}

/// Joined rather than just told to stop, so it has certainly stopped writing
/// to the beat ring before whatever replaces it starts.
impl Drop for SilentDriver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Hands clicks to the app. Sleeps in short steps while playing and parks
/// when stopped, so an idle metronome wakes nothing.
fn beat_loop(shared: Arc<Shared>, notify: Notify) {
    loop {
        if shared.shutdown.load(Ordering::SeqCst) {
            return;
        }
        let mut any = false;
        while let Some(event) = shared.ring.pop() {
            notify(Notice::Beat(event));
            any = true;
        }
        if shared.running.load(Ordering::SeqCst) || any {
            std::thread::sleep(Duration::from_millis(4));
        } else {
            std::thread::park_timeout(Duration::from_millis(500));
        }
    }
}

/// Keeps the device list current on its own thread.
fn scan_loop(
    shared: Arc<Shared>,
    status: Arc<RwLock<Status>>,
    notify: Notify,
    tx: mpsc::Sender<Cmd>,
) {
    loop {
        if shared.shutdown.load(Ordering::SeqCst) {
            return;
        }
        let names = output_device_names();
        let changed = {
            let mut s = status.write();
            if s.devices != names {
                s.devices = names;
                true
            } else {
                false
            }
        };
        if changed {
            notify(Notice::Status);
            let _ = tx.send(Cmd::DevicesChanged);
        }
        std::thread::sleep(SCAN_EVERY);
    }
}

/// Single producer, single consumer, fixed size, no locks.
///
/// The render callback is the only writer and the beat thread the only reader.
/// A full ring drops the newest click rather than blocking the callback; at
/// twenty clicks a second and a reader that wakes every four milliseconds it
/// never fills.
const RING: usize = 64;

struct BeatRing {
    head: AtomicUsize,
    tail: AtomicUsize,
    shape: [AtomicU64; RING],
    at: [AtomicU64; RING],
    tempo: [AtomicU64; RING],
}

impl BeatRing {
    fn new() -> Self {
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            shape: [const { AtomicU64::new(0) }; RING],
            at: [const { AtomicU64::new(0) }; RING],
            tempo: [const { AtomicU64::new(0) }; RING],
        }
    }

    fn push(&self, e: &BeatEvent) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= RING {
            return false;
        }
        let i = head % RING;
        let kind = match e.kind {
            "accent" => 0u64,
            "beat" => 1,
            _ => 2,
        };
        let shape = e.bar as u64
            | (e.beat as u64) << 32
            | (e.sub as u64) << 40
            | kind << 48
            | (e.beats_per_bar as u64) << 56;
        self.shape[i].store(shape, Ordering::Relaxed);
        self.at[i].store(e.at_ms.to_bits(), Ordering::Relaxed);
        self.tempo[i].store(
            e.bpm as u64 | (e.subdivision as u64) << 32,
            Ordering::Relaxed,
        );
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    fn pop(&self) -> Option<BeatEvent> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let i = tail % RING;
        let shape = self.shape[i].load(Ordering::Relaxed);
        let at = f64::from_bits(self.at[i].load(Ordering::Relaxed));
        let tempo = self.tempo[i].load(Ordering::Relaxed);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(BeatEvent {
            bar: shape as u32,
            beat: (shape >> 32) as u8,
            sub: (shape >> 40) as u8,
            kind: match (shape >> 48) as u8 {
                0 => "accent",
                1 => "beat",
                _ => "sub",
            },
            beats_per_bar: (shape >> 56) as u8,
            subdivision: (tempo >> 32) as u8,
            bpm: tempo as u32,
            at_ms: at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(bpm: u32) -> Arc<Shared> {
        let s = MetronomeSettings {
            bpm,
            ..Default::default()
        };
        let shared = Arc::new(Shared {
            running: AtomicBool::new(false),
            generation: AtomicU32::new(0),
            bpm: AtomicU32::new(0),
            beats_per_bar: AtomicU8::new(0),
            subdivision: AtomicU8::new(0),
            accent: AtomicBool::new(false),
            sound: AtomicU8::new(0),
            volume: AtomicU8::new(0),
            ring: BeatRing::new(),
            stats: Stats::default(),
            shutdown: AtomicBool::new(false),
        });
        shared.store(&s);
        shared
    }

    fn play(shared: &Shared) {
        shared.generation.fetch_add(1, Ordering::SeqCst);
        shared.running.store(true, Ordering::SeqCst);
    }

    #[test]
    fn the_ring_carries_every_field_across() {
        let ring = BeatRing::new();
        let e = BeatEvent {
            bar: 70_000,
            beat: 6,
            sub: 3,
            kind: "sub",
            beats_per_bar: 7,
            subdivision: 4,
            bpm: 300,
            at_ms: 1_757_000_000_123.25,
        };
        assert!(ring.push(&e));
        assert_eq!(ring.pop(), Some(e));
        assert_eq!(ring.pop(), None);
    }

    #[test]
    fn a_full_ring_drops_rather_than_blocks() {
        let ring = BeatRing::new();
        let e = BeatEvent {
            bar: 0,
            beat: 0,
            sub: 0,
            kind: "accent",
            beats_per_bar: 4,
            subdivision: 1,
            bpm: 120,
            at_ms: 0.0,
        };
        for _ in 0..RING {
            assert!(ring.push(&e));
        }
        assert!(!ring.push(&e));
        assert!(ring.pop().is_some());
        assert!(ring.push(&e));
    }

    /// Play opens with the downbeat in the first frame, at full level for the
    /// volume, and the event carries the same bar and beat the sound does.
    #[test]
    fn play_sounds_the_downbeat_at_once() {
        let shared = shared(120);
        let mut r = Renderer::new(48_000, shared.clone());
        play(&shared);
        let mut out = vec![0.0f32; 480];
        r.process(480, 1000.0, |i, v| out[i] = v);
        let loud = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(loud > 0.1, "the first block is silent");
        let e = shared.ring.pop().expect("an event for the downbeat");
        assert_eq!((e.bar, e.beat, e.kind), (0, 0, "accent"));
        assert_eq!(e.at_ms, 1000.0);
    }

    #[test]
    fn stopped_means_silent_and_no_events() {
        let shared = shared(120);
        let mut r = Renderer::new(48_000, shared.clone());
        let mut out = vec![1.0f32; 4800];
        r.process(4800, 0.0, |i, v| out[i] = v);
        assert!(out.iter().all(|v| *v == 0.0));
        assert!(shared.ring.pop().is_none());
    }

    #[test]
    fn volume_zero_is_silence_but_time_is_still_kept() {
        let shared = shared(120);
        shared.volume.store(0, Ordering::Relaxed);
        let mut r = Renderer::new(48_000, shared.clone());
        play(&shared);
        let mut out = vec![1.0f32; 48_000];
        r.process(48_000, 0.0, |i, v| out[i] = v);
        assert!(out.iter().all(|v| *v == 0.0));
        let mut n = 0;
        while shared.ring.pop().is_some() {
            n += 1;
        }
        assert_eq!(n, 2, "two beats in a second at 120 bpm");
    }

    /// The event's time follows the click's offset into the block, which is
    /// what lets the interface flash a dot when the sound arrives rather than
    /// when the block was rendered.
    #[test]
    fn an_event_is_stamped_with_when_its_click_is_heard() {
        let shared = shared(120);
        let mut r = Renderer::new(48_000, shared.clone());
        play(&shared);
        r.process(1000, 0.0, |_, _| {});
        let _downbeat = shared.ring.pop();
        // The second beat is at frame 24 000, which is 500 ms in. Render up to
        // just past it in one block that starts at frame 23 000.
        r.process(22_000, 0.0, |_, _| {});
        r.process(2000, 5000.0, |_, _| {});
        let e = shared.ring.pop().expect("second beat");
        assert_eq!(e.beat, 1);
        assert!((e.at_ms - (5000.0 + 1000.0 * 1000.0 / 48_000.0)).abs() < 1e-6);
    }

    /// Overlapping sounds at the fastest grid must never clip, whatever the
    /// volume, and a stack of hi-hat tails is the hardest case there is.
    #[test]
    fn the_fastest_grid_never_clips() {
        let shared = shared(300);
        shared.subdivision.store(4, Ordering::Relaxed);
        shared.volume.store(100, Ordering::Relaxed);
        for sound in 0..Sound::ALL.len() as u8 {
            shared.sound.store(sound, Ordering::Relaxed);
            let mut r = Renderer::new(44_100, shared.clone());
            play(&shared);
            let mut peak = 0.0f32;
            for _ in 0..100 {
                r.process(441, 0.0, |_, v| peak = peak.max(v.abs()));
            }
            assert!(peak < 1.0, "sound {sound} reached {peak}");
            assert!(peak > 0.3, "sound {sound} peaked at only {peak}");
            while shared.ring.pop().is_some() {}
        }
    }

    /// Opens the real default output and keeps time on it for a few seconds,
    /// at volume zero so nothing is heard. Skipped normally, because it needs
    /// a sound device:
    ///
    /// cargo test --manifest-path src-tauri/Cargo.toml --lib live_output -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_output_keeps_time_on_the_default_device() {
        let settings = MetronomeSettings {
            bpm: 240,
            volume: 0,
            ..Default::default()
        };
        let beats = Arc::new(Mutex::new(Vec::<BeatEvent>::new()));
        let seen = beats.clone();
        let engine = Engine::new(
            &settings,
            false,
            Arc::new(move |notice| {
                if let Notice::Beat(b) = notice {
                    seen.lock().push(b);
                }
            }),
        );
        engine.start();
        std::thread::sleep(Duration::from_millis(3000));

        let status = engine.status();
        let stats = engine.stats();
        println!("status: {status:?}");
        println!("stats: {stats:?}");
        assert!(
            status.active_device.is_some(),
            "no device opened: {status:?}"
        );
        assert!(!status.silent);
        assert!(stats.callbacks > 10);
        let ppm = stats
            .device_clock_ppm
            .expect("a clock measurement after warmup");
        assert!(ppm.abs() < 500.0, "device clock off by {ppm} ppm");

        let beats = beats.lock().clone();
        assert!(
            beats.len() >= 10,
            "only {} beats in three seconds at 240",
            beats.len()
        );
        assert_eq!(
            (beats[0].bar, beats[0].beat, beats[0].kind),
            (0, 0, "accent")
        );
        // The stamps are wall time plus latency at each callback, so they
        // carry the callback's jitter; the grid under them is exact.
        let gaps: Vec<f64> = beats.windows(2).map(|w| w[1].at_ms - w[0].at_ms).collect();
        println!("gaps between beats (ms): {gaps:?}");
        // The first gap is skipped: the opening callbacks prefill the device
        // buffer, and the latency they report is not yet the steady one.
        for g in gaps.iter().skip(1) {
            assert!(
                (g - 250.0).abs() < 15.0,
                "a beat {g} ms after the last at 240 bpm"
            );
        }

        engine.stop();
        std::thread::sleep(CLOSE_AFTER + Duration::from_millis(700));
        assert!(
            engine.status().active_device.is_none(),
            "the idle stream was not closed"
        );
        engine.shutdown();
    }

    /// A chosen device that is not there falls back to the default and says
    /// so, rather than staying silent.
    #[test]
    #[ignore]
    fn live_output_falls_back_from_a_missing_device() {
        let settings = MetronomeSettings {
            volume: 0,
            output_device: Some("A device that does not exist".into()),
            ..Default::default()
        };
        let engine = Engine::new(&settings, false, Arc::new(|_| {}));
        engine.start();
        std::thread::sleep(Duration::from_millis(1500));
        let status = engine.status();
        println!("status: {status:?}");
        assert!(status.active_device.is_some());
        assert!(status
            .problem
            .as_deref()
            .is_some_and(|p| p.contains("not found")));
        engine.shutdown();
    }
}
