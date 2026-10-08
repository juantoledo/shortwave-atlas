//! Rig audio for listeners (port of `AudioHub` from `reference/ftdx10_web.py`).
//!
//! One ffmpeg reads the sound card as raw 16-bit mono PCM and its blocks are fanned out
//! to every listener: browsers on `GET /api/audio`, the desktop app over a Tauri channel.
//! ffmpeg runs only while someone listens. A slow listener loses blocks instead of
//! growing a backlog, so latency cannot creep up.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use atlas_core::audio::{
    capture_input_args, parse_asound, parse_avfoundation_devices, parse_dshow_devices, AudioChoice, AudioStatus,
    SoundCard,
};
use atlas_core::platform::Os;
use atlas_rig::process::command;
use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, ChildStderr};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Blocks buffered per listener (1 s at 20 ms per block) before dropping.
const QUEUE_BLOCKS: usize = 50;
/// A listener gets nothing for this long: assume the device is stuck and end the stream.
pub const AUDIO_STALL: Duration = Duration::from_secs(5);
/// ffmpeg stderr lines kept for the settings page.
const LOG_LINES: usize = 30;

struct Inner {
    choice: AudioChoice,
    subs: Vec<(u64, mpsc::Sender<Bytes>)>,
    next_id: u64,
    /// Reads ffmpeg; aborting it drops (and so kills) the child.
    pump: Option<JoinHandle<()>>,
    /// Bumped on every start and reconfigure, so a stale pump cannot touch the state.
    generation: u64,
    running: bool,
    last_error: Option<String>,
    spawn_error: Option<String>,
    log: VecDeque<String>,
}

impl Inner {
    fn stop(&mut self) {
        if let Some(p) = self.pump.take() {
            p.abort();
        }
        self.running = false;
    }
}

pub struct AudioHub {
    ffmpeg: String,
    inner: Mutex<Inner>,
}

#[derive(Debug)]
pub enum OpenError {
    Disabled,
    /// ffmpeg could not be started.
    Spawn(std::io::Error),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "audio is disabled"),
            Self::Spawn(e) => write!(f, "could not start ffmpeg: {e}"),
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
    /// The next block, or `None` when the stream ended (ffmpeg stopped, the audio was
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
    pub fn new(ffmpeg: &str, choice: AudioChoice) -> Arc<Self> {
        Arc::new(Self {
            ffmpeg: ffmpeg.into(),
            inner: Mutex::new(Inner {
                choice,
                subs: vec![],
                next_id: 0,
                pump: None,
                generation: 0,
                running: false,
                last_error: None,
                spawn_error: None,
                log: VecDeque::new(),
            }),
        })
    }

    /// Bytes per block sent to listeners (20 ms).
    fn chunk(rate: u32) -> usize {
        rate as usize * 2 / 50
    }

    fn ffmpeg_args(os: Os, choice: &AudioChoice) -> Vec<String> {
        let head = ["-hide_banner", "-loglevel", "error", "-nostdin", "-fflags", "nobuffer"];
        let rate = choice.rate.to_string();
        let tail = ["-ac", "1", "-ar", rate.as_str(), "-f", "s16le", "-flush_packets", "1", "pipe:1"];
        head.iter()
            .map(|s| s.to_string())
            .chain(capture_input_args(os, &choice.device))
            .chain(tail.iter().map(|s| s.to_string()))
            .collect()
    }

    pub fn subscribe(self: &Arc<Self>) -> Result<Subscription, OpenError> {
        let mut inner = self.inner.lock().expect("audio lock");
        if !inner.choice.enabled {
            return Err(OpenError::Disabled);
        }
        if inner.pump.is_none() {
            let spawned = command(&self.ffmpeg)
                .args(Self::ffmpeg_args(Os::CURRENT, &inner.choice))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn();
            let child = match spawned {
                Ok(c) => c,
                Err(e) => {
                    inner.spawn_error = Some(e.to_string());
                    return Err(OpenError::Spawn(e));
                }
            };
            inner.generation += 1;
            inner.spawn_error = None;
            inner.log.clear();
            let chunk = Self::chunk(inner.choice.rate);
            inner.pump = Some(tokio::spawn(pump(Arc::downgrade(self), child, inner.generation, chunk)));
        }
        let (tx, rx) = mpsc::channel(QUEUE_BLOCKS);
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

    /// Switch to a new choice: stop ffmpeg and end every stream. Listeners reconnect and
    /// get the new device and rate (or a refusal, if audio is now disabled).
    pub fn reconfigure(&self, choice: AudioChoice) {
        let mut inner = self.inner.lock().expect("audio lock");
        inner.stop();
        inner.subs.clear(); // dropping the senders closes the streams
        inner.generation += 1;
        inner.choice = choice;
        inner.last_error = None;
        inner.spawn_error = None;
        inner.log.clear();
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
            spawn_error: inner.spawn_error.clone(),
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
        }
        for (_, tx) in &inner.subs {
            // full = slow listener: drop the block, the page resyncs
            let _ = tx.try_send(block.clone());
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

    /// ffmpeg exited on its own (device busy/unplugged): record why and end every stream.
    fn ended(&self, generation: u64, exit: String) {
        let mut inner = self.inner.lock().expect("audio lock");
        if inner.generation != generation {
            return;
        }
        // the ALSA line says why ("cannot open audio device ... (Device or resource busy)")
        let why = inner
            .log
            .iter()
            .rev()
            .find(|l| l.contains("cannot open audio device"))
            .or(inner.log.back())
            .cloned()
            .unwrap_or(exit);
        tracing::warn!("ffmpeg stopped: {why}");
        inner.last_error = Some(why);
        inner.pump = None;
        inner.running = false;
        inner.subs.clear();
    }
}

async fn read_stderr(hub: Weak<AudioHub>, err: ChildStderr, generation: u64) {
    let mut lines = BufReader::new(err).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Some(h) = hub.upgrade() else { return };
        h.log_line(generation, line);
    }
}

async fn pump(hub: Weak<AudioHub>, mut child: Child, generation: u64, chunk: usize) {
    let mut out = child.stdout.take().expect("piped stdout");
    let stderr = tokio::spawn(read_stderr(hub.clone(), child.stderr.take().expect("piped stderr"), generation));
    let mut buf = BytesMut::with_capacity(chunk * 2);
    let mut read = vec![0u8; 4096];
    loop {
        let n = match out.read(&mut read).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&read[..n]);
        if buf.len() < chunk {
            continue;
        }
        let Some(h) = hub.upgrade() else { return };
        h.fan_out(generation, buf.split().freeze());
    }
    let exit = match child.wait().await {
        Ok(s) => format!("ffmpeg exited: {s}"),
        Err(e) => format!("ffmpeg: {e}"),
    };
    let _ = stderr.await; // so the reason is in the log before we look for it
    if let Some(h) = hub.upgrade() {
        h.ended(generation, exit);
    }
}

/// Capture-capable sound cards on this machine: `/proc/asound` on Linux; on Windows and
/// macOS, what ffmpeg's DirectShow / AVFoundation input lists (empty without ffmpeg).
pub async fn sound_cards(ffmpeg: &str) -> Vec<SoundCard> {
    let list = |args: &'static [&'static str]| async move {
        match command(ffmpeg).args(args).output().await {
            // the list goes to stderr, and ffmpeg exits with an error after printing it
            Ok(out) => String::from_utf8_lossy(&out.stderr).into_owned(),
            Err(e) => {
                tracing::warn!("ffmpeg -list_devices: {e}");
                String::new()
            }
        }
    };
    match Os::CURRENT {
        Os::Linux => alsa_cards(Path::new("/")),
        Os::Windows => {
            parse_dshow_devices(&list(&["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]).await)
        }
        Os::Macos => parse_avfoundation_devices(
            &list(&["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""]).await,
        ),
    }
}

/// Linux: capture-capable ALSA devices under `root` (`/` in production; a temp tree in tests).
pub fn alsa_cards(root: &Path) -> Vec<SoundCard> {
    let read = |name: &str| std::fs::read_to_string(root.join("proc/asound").join(name)).unwrap_or_default();
    parse_asound(&read("cards"), &read("pcm"))
}

/// `ffmpeg -version` (e.g. "ffmpeg version 6.1.1-3ubuntu5 ..."), or `None` if it cannot run.
pub async fn ffmpeg_version(ffmpeg: &str) -> Option<String> {
    let out = command(ffmpeg).arg("-version").output().await.ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec(rate: u32) -> AudioChoice {
        AudioChoice { enabled: true, device: "plughw:CARD=CODEC,DEV=0".into(), rate }
    }

    #[test]
    fn ffmpeg_command_matches_the_prototype() {
        let a = AudioHub::ffmpeg_args(Os::Linux, &codec(16_000)).join(" ");
        assert!(a.contains("-f alsa -ac 2 -ar 48000 -i plughw:CARD=CODEC,DEV=0"));
        assert!(a.ends_with("-ac 1 -ar 16000 -f s16le -flush_packets 1 pipe:1"));
        assert_eq!(AudioHub::chunk(16_000), 640);
    }

    #[tokio::test]
    async fn disabled_audio_refuses_listeners() {
        let hub = AudioHub::new("ffmpeg", AudioChoice { enabled: false, ..codec(16_000) });
        assert!(matches!(hub.subscribe(), Err(OpenError::Disabled)));
    }

    #[tokio::test]
    async fn missing_ffmpeg_is_reported() {
        let hub = AudioHub::new("/nonexistent/ffmpeg-swatlas", codec(16_000));
        assert!(matches!(hub.subscribe(), Err(OpenError::Spawn(_))));
        assert!(hub.status().spawn_error.is_some());
        hub.reconfigure(codec(8_000));
        assert_eq!(hub.status().spawn_error, None);
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

    /// Real ffmpeg on ALSA's `null` device (endless silence); skipped without ffmpeg.
    /// Linux only: the device names are ALSA's.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn real_ffmpeg_streams_and_reconfigures() {
        if ffmpeg_version("ffmpeg").await.is_none() {
            eprintln!("ffmpeg not installed: skipping");
            return;
        }
        let hub = AudioHub::new("ffmpeg", AudioChoice { enabled: true, device: "null".into(), rate: 8_000 });
        let mut a = hub.subscribe().unwrap();
        let mut b = hub.subscribe().unwrap();
        assert_eq!(a.rate, 8_000);
        let block = a.next_block().await.expect("PCM from ffmpeg");
        assert!(block.len() >= AudioHub::chunk(8_000));
        assert!(b.next_block().await.is_some());
        let st = hub.status();
        assert!(st.running);
        assert_eq!(st.listeners, 2);

        // a new choice ends the current streams
        hub.reconfigure(AudioChoice { enabled: true, device: "null".into(), rate: 16_000 });
        while a.next_block().await.is_some() {}
        assert_eq!(hub.status().listeners, 0);
        drop((a, b));
        let mut c = hub.subscribe().unwrap();
        assert!(c.next_block().await.is_some());
        drop(c);
        assert!(!hub.status().running);

        // a card that is not there: ffmpeg stops and says why
        hub.reconfigure(AudioChoice { enabled: true, device: "plughw:CARD=NoSuchCard,DEV=0".into(), rate: 16_000 });
        let mut d = hub.subscribe().unwrap();
        assert!(d.next_block().await.is_none());
        let st = hub.status();
        let err = st.last_error.unwrap();
        assert!(err.contains("cannot open audio device"), "{err}");
        assert_eq!(
            atlas_core::audio::diagnose_audio(Os::Linux, &hub.status()),
            [atlas_core::audio::AudioHint::DeviceMissing]
        );
    }
}
