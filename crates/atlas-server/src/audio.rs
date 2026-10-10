//! Rig audio for listeners (port of `AudioHub` from the original Python prototype).
//!
//! One capture (cpal: ALSA, WASAPI or CoreAudio) reads the sound card; its samples become
//! 16-bit mono PCM blocks (`atlas_core::audio::Converter`) fanned out to every listener:
//! browsers on `GET /api/audio`, the desktop app over a Tauri channel. The capture runs only
//! while someone listens. A slow listener loses blocks instead of growing a backlog, so
//! latency cannot creep up.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use atlas_core::audio::{
    cards_from_names, find_device, parse_asound, pick_capture_format, rate_drift, AudioChoice, AudioStatus,
    AudioTuning, CaptureFailure, CaptureFormat, CaptureOffer, Converter, SampleKind, SoundCard,
};
use atlas_core::platform::Os;
use bytes::Bytes;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use tokio::sync::mpsc;

/// A listener gets nothing for this long: assume the device is stuck and end the stream.
pub const AUDIO_STALL: Duration = Duration::from_secs(5);
/// Capture log lines kept for the settings page.
const LOG_LINES: usize = 30;
/// How long after the first block the delivered rate is checked against the declared one.
const DRIFT_CHECK: Duration = Duration::from_secs(10);
/// Only exact zeros for this long: flagged as silent (macOS gives a denied app silence).
const SILENCE: Duration = Duration::from_secs(3);
/// How often the capture thread looks at its stop flag.
const POLL: Duration = Duration::from_millis(100);

/// A running capture thread; it owns the cpal stream and stops it when `stop` is set.
struct Capture {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

struct Inner {
    choice: AudioChoice,
    subs: Vec<(u64, mpsc::Sender<Bytes>)>,
    next_id: u64,
    capture: Option<Capture>,
    /// Bumped on every start and reconfigure, so a stale capture cannot touch the state.
    generation: u64,
    running: bool,
    silent: bool,
    last_error: Option<String>,
    failure: Option<CaptureFailure>,
    log: VecDeque<String>,
}

impl Inner {
    /// Tell the capture thread to stop (it exits within `POLL`); the thread, to wait for.
    fn stop(&mut self) -> Option<JoinHandle<()>> {
        self.running = false;
        self.silent = false;
        let c = self.capture.take()?;
        c.stop.store(true, Ordering::Relaxed);
        Some(c.thread)
    }
}

pub struct AudioHub {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
pub enum OpenError {
    Disabled,
    /// The capture thread could not be started.
    Spawn(std::io::Error),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "audio is disabled"),
            Self::Spawn(e) => write!(f, "could not start the audio capture: {e}"),
        }
    }
}

/// A listener's stream; dropping it unsubscribes.
pub struct Subscription {
    /// Sample rate of this stream's PCM (a reconfigure ends the stream first).
    pub rate: u32,
    rx: mpsc::Receiver<Bytes>,
    id: u64,
    hub: Weak<AudioHub>,
}

impl Subscription {
    /// The next block, or `None` when the stream ended (the capture stopped, the audio was
    /// reconfigured, or nothing arrived for `AUDIO_STALL`).
    pub async fn next_block(&mut self) -> Option<Bytes> {
        tokio::time::timeout(AUDIO_STALL, self.rx.recv()).await.ok().flatten()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(hub) = self.hub.upgrade() {
            hub.unsubscribe(self.id);
        }
    }
}

impl AudioHub {
    pub fn new(choice: AudioChoice) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                choice,
                subs: vec![],
                next_id: 0,
                capture: None,
                generation: 0,
                running: false,
                silent: false,
                last_error: None,
                failure: None,
                log: VecDeque::new(),
            }),
        })
    }

    /// Bytes per block sent to listeners (`block_ms` of s16le mono).
    fn chunk(rate: u32, t: &AudioTuning) -> usize {
        rate as usize * 2 * t.block_ms as usize / 1000
    }

    /// Blocks buffered per listener (`queue_ms`) before dropping.
    fn queue_blocks(t: &AudioTuning) -> usize {
        (t.queue_ms.div_ceil(t.block_ms.max(1)) as usize).max(2)
    }

    pub fn subscribe(self: &Arc<Self>) -> Result<Subscription, OpenError> {
        let mut inner = self.inner.lock().expect("audio lock");
        if !inner.choice.enabled {
            return Err(OpenError::Disabled);
        }
        if inner.capture.is_none() {
            inner.generation += 1;
            let (hub, generation, choice) = (Arc::downgrade(self), inner.generation, inner.choice.clone());
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let thread = std::thread::Builder::new()
                .name("audio-capture".into())
                .spawn(move || run_capture(hub, generation, choice, flag))
                .map_err(OpenError::Spawn)?;
            inner.capture = Some(Capture { stop, thread });
            inner.last_error = None;
            inner.failure = None;
            inner.log.clear();
        }
        let (tx, rx) = mpsc::channel(Self::queue_blocks(&inner.choice.tuning));
        let id = inner.next_id;
        inner.next_id += 1;
        inner.subs.push((id, tx));
        Ok(Subscription { rate: inner.choice.rate, rx, id, hub: Arc::downgrade(self) })
    }

    fn unsubscribe(&self, id: u64) {
        let mut inner = self.inner.lock().expect("audio lock");
        inner.subs.retain(|(i, _)| *i != id);
        if inner.subs.is_empty() {
            inner.stop();
        }
    }

    /// Switch to a new choice: stop the capture and end every stream. Listeners reconnect
    /// and get the new device and rate (or a refusal, if audio is now disabled).
    pub fn reconfigure(&self, choice: AudioChoice) {
        let mut inner = self.inner.lock().expect("audio lock");
        inner.stop();
        inner.subs.clear(); // dropping the senders closes the streams
        inner.generation += 1;
        inner.choice = choice;
        inner.last_error = None;
        inner.failure = None;
        inner.log.clear();
    }

    /// Stop the capture and end every stream, waiting until the device is closed (app exit).
    pub async fn shutdown(&self) {
        let thread = {
            let mut inner = self.inner.lock().expect("audio lock");
            inner.subs.clear();
            inner.generation += 1;
            inner.stop()
        };
        if let Some(t) = thread {
            let _ = tokio::task::spawn_blocking(move || t.join()).await;
        }
    }

    pub fn choice(&self) -> AudioChoice {
        self.inner.lock().expect("audio lock").choice.clone()
    }

    pub fn status(&self) -> AudioStatus {
        let inner = self.inner.lock().expect("audio lock");
        AudioStatus {
            enabled: inner.choice.enabled,
            running: inner.running,
            listeners: inner.subs.len() as u32,
            last_error: inner.last_error.clone(),
            failure: inner.failure,
            silent: inner.silent,
            log: inner.log.iter().cloned().collect(),
        }
    }

    fn fan_out(&self, generation: u64, block: Bytes) {
        let mut inner = self.inner.lock().expect("audio lock");
        if inner.generation != generation {
            return;
        }
        if !inner.running {
            inner.running = true;
            inner.last_error = None;
            inner.failure = None;
        }
        for (_, tx) in &inner.subs {
            // full = slow listener: drop the block, the page resyncs
            let _ = tx.try_send(block.clone());
        }
    }

    fn set_silent(&self, generation: u64, silent: bool) {
        let mut inner = self.inner.lock().expect("audio lock");
        if inner.generation == generation {
            inner.silent = silent;
        }
    }

    fn log_line(&self, generation: u64, line: String) {
        let mut inner = self.inner.lock().expect("audio lock");
        if inner.generation == generation {
            if inner.log.len() == LOG_LINES {
                inner.log.pop_front();
            }
            inner.log.push_back(line);
        }
    }

    /// The capture stopped on its own (device busy, unplugged, silent for too long): record
    /// why and end every stream.
    fn ended(&self, generation: u64, why: String, failure: CaptureFailure) {
        let mut inner = self.inner.lock().expect("audio lock");
        if inner.generation != generation {
            return;
        }
        tracing::warn!("audio capture stopped: {why}");
        inner.last_error = Some(why);
        inner.failure = Some(failure);
        inner.capture = None; // the thread is returning
        inner.running = false;
        inner.silent = false;
        inner.subs.clear();
    }
}

/// What the cpal callbacks send the capture thread.
enum Event {
    /// Interleaved samples, as f32.
    Data(Vec<f32>),
    Error(cpal::Error),
}

/// The capture thread: open the device, convert what it delivers and fan it out, until
/// `stop` is set (dropping the stream closes the device) or the capture fails.
fn run_capture(hub: Weak<AudioHub>, generation: u64, choice: AudioChoice, stop: Arc<AtomicBool>) {
    let with_hub = |f: &dyn Fn(&AudioHub)| {
        if let Some(h) = hub.upgrade() {
            f(&h);
        }
    };
    let end = |why: String, failure| with_hub(&|h| h.ended(generation, why.clone(), failure));
    let (tx, rx) = sync_channel::<Event>(64);
    let (_stream, name, fmt) = match open(&choice, tx) {
        Ok(opened) => opened,
        Err((why, failure)) => return end(why, failure),
    };
    let mut converter = match Converter::new(fmt.channels, fmt.rate, choice.rate) {
        Ok(c) => c,
        Err(why) => return end(why, CaptureFailure::Other),
    };
    let line = format!("{name}: {} ch, {} Hz, {:?} -> {} Hz mono", fmt.channels, fmt.rate, fmt.format, choice.rate);
    tracing::info!("audio capture: {line}");
    with_hub(&|h| h.log_line(generation, line.clone()));

    let chunk = AudioHub::chunk(choice.rate, &choice.tuning);
    let mut pcm: Vec<u8> = Vec::with_capacity(chunk * 2);
    let mut last_data = Instant::now();
    // bytes after the first block, checked once against the rate (a short stream cuts out)
    let (mut first, mut since_first, mut drift_checked) = (None::<Instant>, 0u64, false);
    let (mut heard, mut flagged_silent) = (false, false);
    while !stop.load(Ordering::Relaxed) {
        match rx.recv_timeout(POLL) {
            Ok(Event::Data(samples)) => {
                last_data = Instant::now();
                let start = *first.get_or_insert(last_data);
                if !heard && samples.iter().any(|s| *s != 0.0) {
                    heard = true;
                    if flagged_silent {
                        with_hub(&|h| h.set_silent(generation, false));
                    }
                } else if !heard && !flagged_silent && start.elapsed() >= SILENCE {
                    flagged_silent = true;
                    with_hub(&|h| h.set_silent(generation, true));
                }
                let before = pcm.len();
                converter.push(&samples, &mut pcm);
                if !drift_checked {
                    since_first += (pcm.len() - before) as u64;
                    if start.elapsed() >= DRIFT_CHECK {
                        drift_checked = true;
                        if let Some(line) = rate_drift(since_first, start.elapsed(), choice.rate) {
                            tracing::warn!("audio capture {line}");
                            with_hub(&|h| h.log_line(generation, line.clone()));
                        }
                    }
                }
                if pcm.len() >= chunk {
                    let block = Bytes::from(std::mem::replace(&mut pcm, Vec::with_capacity(chunk * 2)));
                    with_hub(&|h| h.fan_out(generation, block.clone()));
                }
            }
            Ok(Event::Error(e)) => match e.kind() {
                // the stream goes on
                cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied => {
                    with_hub(&|h| h.log_line(generation, e.to_string()));
                }
                _ => return end(format!("{name}: {e}"), failure_of(&e)),
            },
            Err(RecvTimeoutError::Timeout) if last_data.elapsed() >= AUDIO_STALL => {
                return end(format!("{name}: no audio for {} s", AUDIO_STALL.as_secs()), CaptureFailure::Other);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return end(format!("{name}: the stream closed"), CaptureFailure::Other),
        }
    }
}

/// Open and start the configured device: the stream (capturing while it lives), the
/// device's name and the format it runs in.
fn open(
    choice: &AudioChoice,
    tx: SyncSender<Event>,
) -> Result<(cpal::Stream, String, CaptureFormat), (String, CaptureFailure)> {
    let fail = |what: &str, e: cpal::Error| (format!("{}: {what}: {e}", choice.device), failure_of(&e));
    let host = cpal::default_host();
    let devices: Vec<cpal::Device> = host.input_devices().map_err(|e| fail("listing devices", e))?.collect();
    let keys: Vec<String> = devices.iter().map(|d| device_key(Os::CURRENT, d)).collect();
    let Some(i) = find_device(Os::CURRENT, &choice.device, &keys) else {
        return Err((format!("{}: no such capture device", choice.device), CaptureFailure::DeviceMissing));
    };
    let (dev, name) = (&devices[i], keys[i].clone());
    let offers: Vec<CaptureOffer> = dev
        .supported_input_configs()
        .map_err(|e| fail("reading its formats", e))?
        .map(|c| CaptureOffer {
            channels: c.channels(),
            min_rate: c.min_sample_rate(),
            max_rate: c.max_sample_rate(),
            format: sample_kind(c.sample_format()),
        })
        .collect();
    let default = dev.default_input_config().ok().map(|c| CaptureFormat {
        channels: c.channels(),
        rate: c.sample_rate(),
        format: sample_kind(c.sample_format()),
    });
    let Some(fmt) = pick_capture_format(&offers, choice.tuning.capture_rate, default) else {
        return Err((format!("{name}: no sample format this program can read"), CaptureFailure::Other));
    };
    let config = cpal::StreamConfig { channels: fmt.channels, sample_rate: fmt.rate, buffer_size: cpal::BufferSize::Default };
    let stream = match fmt.format {
        SampleKind::F32 => build::<f32>(dev, config, tx),
        SampleKind::I16 => build::<i16>(dev, config, tx),
        SampleKind::I32 => build::<i32>(dev, config, tx),
        SampleKind::U16 => build::<u16>(dev, config, tx),
        SampleKind::Other => unreachable!("pick_capture_format never picks it"),
    }
    .map_err(|e| fail("opening it", e))?;
    stream.play().map_err(|e| fail("starting it", e))?;
    Ok((stream, name, fmt))
}

/// An input stream that sends its samples, as f32, and its errors to `tx`. A full channel
/// (the capture thread is behind) drops the samples rather than blocking the audio thread.
fn build<T>(dev: &cpal::Device, config: cpal::StreamConfig, tx: SyncSender<Event>) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let errors = tx.clone();
    dev.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let _ = tx.try_send(Event::Data(data.iter().map(|&s| s.to_sample::<f32>()).collect()));
        },
        move |e| {
            let _ = errors.try_send(Event::Error(e));
        },
        None,
    )
}

fn sample_kind(f: SampleFormat) -> SampleKind {
    match f {
        SampleFormat::F32 => SampleKind::F32,
        SampleFormat::I16 => SampleKind::I16,
        SampleFormat::I32 => SampleKind::I32,
        SampleFormat::U16 => SampleKind::U16,
        _ => SampleKind::Other,
    }
}

fn failure_of(e: &cpal::Error) -> CaptureFailure {
    match e.kind() {
        cpal::ErrorKind::DeviceBusy => CaptureFailure::DeviceBusy,
        cpal::ErrorKind::DeviceNotAvailable => CaptureFailure::DeviceMissing,
        cpal::ErrorKind::PermissionDenied => CaptureFailure::PermissionDenied,
        _ => CaptureFailure::Other,
    }
}

/// How the config names a device: its ALSA PCM name on Linux, its name elsewhere.
fn device_key(os: Os, d: &cpal::Device) -> String {
    match os {
        Os::Linux => d.id().map(|id| id.id().to_string()),
        Os::Windows | Os::Macos => d.description().map(|desc| desc.name().to_string()),
    }
    .unwrap_or_default()
}

/// Capture-capable sound cards on this machine: `/proc/asound` on Linux (plughw names by
/// card id, which survive re-plugging); on Windows and macOS, the input devices the OS lists.
pub async fn sound_cards() -> Vec<SoundCard> {
    match Os::CURRENT {
        Os::Linux => alsa_cards(Path::new("/")),
        os => tokio::task::spawn_blocking(move || {
            let names = match cpal::default_host().input_devices() {
                Ok(devices) => devices.map(|d| device_key(os, &d)).collect(),
                Err(e) => {
                    tracing::warn!("listing sound cards: {e}");
                    vec![]
                }
            };
            cards_from_names(os, names)
        })
        .await
        .unwrap_or_default(),
    }
}

/// Linux: capture-capable ALSA devices under `root` (`/` in production; a temp tree in tests).
pub fn alsa_cards(root: &Path) -> Vec<SoundCard> {
    let read = |name: &str| std::fs::read_to_string(root.join("proc/asound").join(name)).unwrap_or_default();
    parse_asound(&read("cards"), &read("pcm"))
}

/// The OS audio API the capture uses ("ALSA", "WASAPI", "CoreAudio").
pub fn backend() -> String {
    cpal::default_host().id().name().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::audio::default_tuning;

    fn codec(rate: u32) -> AudioChoice {
        AudioChoice { enabled: true, device: "plughw:CARD=CODEC,DEV=0".into(), rate, tuning: default_tuning(Os::Linux) }
    }

    #[test]
    fn blocks_and_queue_follow_the_tuning() {
        let t = default_tuning(Os::Linux);
        assert_eq!(AudioHub::chunk(16_000, &t), 640);
        assert_eq!(AudioHub::queue_blocks(&t), 50);
        let t = AudioTuning { block_ms: 40, queue_ms: 500, ..default_tuning(Os::Macos) };
        assert_eq!(AudioHub::chunk(16_000, &t), 1_280);
        assert_eq!(AudioHub::queue_blocks(&t), 13); // rounded up
        assert_eq!(AudioHub::queue_blocks(&AudioTuning { block_ms: 100, queue_ms: 100, ..t }), 2);
    }

    #[tokio::test]
    async fn disabled_audio_refuses_listeners() {
        let hub = AudioHub::new(AudioChoice { enabled: false, ..codec(16_000) });
        assert!(matches!(hub.subscribe(), Err(OpenError::Disabled)));
    }

    /// Any machine, sound cards or not: a device that is not there ends the stream and
    /// says why; a new choice clears it.
    #[tokio::test]
    async fn missing_device_is_reported() {
        let hub = AudioHub::new(AudioChoice { device: "No Such Card 7".into(), ..codec(16_000) });
        let mut sub = hub.subscribe().unwrap();
        assert!(sub.next_block().await.is_none());
        let st = hub.status();
        assert!(!st.running && st.listeners == 0);
        assert!(st.last_error.unwrap().starts_with("No Such Card 7: "));
        assert!(st.failure.is_some());
        hub.reconfigure(codec(8_000));
        assert_eq!((hub.status().last_error, hub.status().failure), (None, None));
        drop(sub);
        hub.shutdown().await;
    }

    #[cfg(unix)]
    #[test]
    fn lists_cards_from_proc() {
        let root = std::env::temp_dir().join(format!("swatlas-asound-{}", std::process::id()));
        let dir = root.join("proc/asound");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cards"), " 1 [CODEC          ]: USB-Audio - USB AUDIO  CODEC\n   BurrBrown\n")
            .unwrap();
        std::fs::write(dir.join("pcm"), "01-00: USB Audio : USB Audio : playback 1 : capture 1\n").unwrap();
        let cards = alsa_cards(&root);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].device, "plughw:CARD=CODEC,DEV=0");
        assert!(alsa_cards(Path::new("/nonexistent-root")).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    /// A real capture from ALSA's `null` device (endless silence); skipped where ALSA does
    /// not list it. Linux only: the device names are ALSA's.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn real_capture_streams_and_reconfigures() {
        let listed = cpal::default_host()
            .input_devices()
            .map(|mut d| d.any(|d| device_key(Os::Linux, &d) == "null"))
            .unwrap_or(false);
        if !listed {
            eprintln!("ALSA lists no null device: skipping");
            return;
        }
        let hub = AudioHub::new(AudioChoice { device: "null".into(), ..codec(8_000) });
        let mut a = hub.subscribe().unwrap();
        let mut b = hub.subscribe().unwrap();
        assert_eq!(a.rate, 8_000);
        let block = a.next_block().await.expect("PCM from the capture");
        assert!(block.len() >= AudioHub::chunk(8_000, &default_tuning(Os::Linux)));
        assert!(b.next_block().await.is_some());
        let st = hub.status();
        assert!(st.running);
        assert_eq!(st.listeners, 2);

        // a new choice ends the current streams
        hub.reconfigure(AudioChoice { device: "null".into(), ..codec(16_000) });
        while a.next_block().await.is_some() {}
        assert_eq!(hub.status().listeners, 0);
        drop((a, b));
        let mut c = hub.subscribe().unwrap();
        assert_eq!(c.rate, 16_000);
        assert!(c.next_block().await.is_some());
        drop(c);
        assert!(!hub.status().running);
        hub.shutdown().await;
    }
}
