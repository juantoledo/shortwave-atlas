//! An internet station as the audio hub's source (the NET band): its stream fetched, decoded
//! (symphonia: MP3, AAC, Ogg Vorbis, FLAC) to 16-bit mono PCM at `NET_RATE` with the rig
//! capture's `Converter`, and paced at real time by a `JitterBuffer`, so listeners get it in the
//! same blocks and on the same transport as the rig's audio. The PCM is relayed live and never
//! written anywhere.
//!
//! Three parts, stopped together by the hub: a task reading the stream, a thread decoding it,
//! and a task handing one block out every `block_ms`.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use atlas_core::audio::Converter;
use atlas_core::net::{JitterBuffer, NetStream, NET_RATE};
use bytes::Bytes;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use tokio::sync::mpsc;

use crate::audio::{AudioHub, Capture, OpenError};

/// No bytes from the stream for this long: it's gone.
const STALL: Duration = Duration::from_secs(15);
/// Stream chunks in flight between the reader and the decoder.
const CHUNKS: usize = 64;

/// Start playing `stream` into `hub` for the hub's `generation`, holding `buffer` milliseconds
/// ahead (`None`: sized automatically).
pub(crate) fn start(
    hub: Weak<AudioHub>,
    generation: u64,
    stream: NetStream,
    block_ms: u32,
    buffer: Option<u32>,
) -> Result<Capture, OpenError> {
    let rt = tokio::runtime::Handle::try_current()
        .map_err(|_| OpenError::Spawn(std::io::Error::other("no async runtime")))?;
    let (tx, rx) = mpsc::channel::<Bytes>(CHUNKS);
    let stop = Arc::new(AtomicBool::new(false));
    let jitter = Arc::new(Mutex::new(JitterBuffer::new(NET_RATE, block_ms, buffer)));
    let held = jitter.clone();

    let (url, h) = (stream.url.clone(), hub.clone());
    let fetch = rt.spawn(async move {
        if let Err(e) = crate::http::stream(&url, STALL, tx).await {
            if let Some(h) = h.upgrade() {
                h.net_ended(generation, e);
            }
        }
    });

    let (h, jb) = (hub.clone(), jitter.clone());
    let pacer = rt.spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(u64::from(block_ms.max(1))));
        loop {
            tick.tick().await;
            let block = {
                let mut j = jb.lock().expect("jitter buffer");
                let before = j.target_ms();
                let block = j.pop();
                if j.target_ms() != before {
                    tracing::info!(
                        "internet station ran dry ({} times): buffering {} ms now",
                        j.underruns(),
                        j.target_ms()
                    );
                }
                block
            };
            if let Some(b) = block {
                let Some(hub) = h.upgrade() else { return };
                hub.fan_out(generation, Bytes::from(b));
            }
        }
    });

    let (flag, codec) = (stop.clone(), stream.station.codec.clone());
    let thread = std::thread::Builder::new()
        .name("net-audio".into())
        .spawn(move || {
            let r =
                decode(Chunks::new(rx), codec.as_deref(), &flag, |pcm| jitter.lock().expect("jitter buffer").push(pcm));
            if let (Err(e), false) = (r, flag.load(Ordering::Relaxed)) {
                if let Some(h) = hub.upgrade() {
                    h.net_ended(generation, e);
                }
            }
        })
        .map_err(OpenError::Spawn)?;
    Ok(Capture { stop, thread, tasks: vec![fetch.abort_handle(), pacer.abort_handle()], jitter: Some(held) })
}

/// The stream's bytes as `Read`, from the reader task; end of file once it is gone.
struct Chunks {
    rx: mpsc::Receiver<Bytes>,
    cur: Bytes,
}

impl Chunks {
    fn new(rx: mpsc::Receiver<Bytes>) -> Self {
        Self { rx, cur: Bytes::new() }
    }
}

impl Read for Chunks {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        while self.cur.is_empty() {
            match self.rx.blocking_recv() {
                Some(b) => self.cur = b,
                None => return Ok(0),
            }
        }
        let n = buf.len().min(self.cur.len());
        buf[..n].copy_from_slice(&self.cur.split_to(n));
        Ok(n)
    }
}

/// Decode a stream in any enabled format (`codec`, as the directory names it, is a hint) into
/// 16-bit mono PCM at `NET_RATE`, handed to `out` as it comes, until `stop` is set (`Ok`) or the
/// stream ends or can't be decoded (`Err`, saying why).
pub(crate) fn decode(
    reader: impl Read + Send + Sync + 'static,
    codec: Option<&str>,
    stop: &AtomicBool,
    mut out: impl FnMut(&[u8]),
) -> Result<(), String> {
    let mut hint = Hint::new();
    match codec.map(str::to_ascii_lowercase).as_deref() {
        Some("mp3") => hint.with_extension("mp3"),
        Some("aac" | "aac+") => hint.with_extension("aac"),
        Some("ogg" | "vorbis") => hint.with_extension("ogg"),
        Some("flac") => hint.with_extension("flac"),
        _ => &mut hint,
    };
    let mss = MediaSourceStream::new(Box::new(ReadOnlySource::new(reader)), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("not a stream that can be played here: {e}"))?;
    let track = format.default_track(TrackType::Audio).ok_or("the stream has no audio")?;
    let track_id = track.id;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or("the stream has no audio")?.clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| format!("this stream's format can't be played: {e}"))?;

    let mut conv: Option<(u16, u32, Converter)> = None;
    let (mut samples, mut pcm) = (Vec::<f32>::new(), Vec::<u8>::new());
    while !stop.load(Ordering::Relaxed) {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => return Err("the stream ended".into()),
            Err(SymError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err("the stream ended".into())
            }
            Err(e) => return Err(format!("the stream broke: {e}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            // a damaged frame: skip it, the next one usually decodes
            Err(SymError::DecodeError(_) | SymError::IoError(_)) => continue,
            Err(e) => return Err(format!("the stream can't be decoded: {e}")),
        };
        let (rate, channels) = (buf.spec().rate(), buf.spec().channels().count() as u16);
        if buf.frames() == 0 || channels == 0 {
            continue;
        }
        if conv.as_ref().is_none_or(|(c, r, _)| (*c, *r) != (channels, rate)) {
            conv = Some((channels, rate, Converter::new(channels, rate, NET_RATE)?));
        }
        buf.copy_to_vec_interleaved(&mut samples);
        pcm.clear();
        conv.as_mut().expect("set above").2.push(&samples, &mut pcm);
        if !pcm.is_empty() {
            out(&pcm);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A WAV file: `secs` of a 440 Hz tone, 16-bit stereo at 44.1 kHz.
    fn wav(secs: f32) -> Vec<u8> {
        let (rate, ch) = (44_100u32, 2u16);
        let frames = (rate as f32 * secs) as u32;
        let data = frames * u32::from(ch) * 2;
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&ch.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * u32::from(ch) * 2).to_le_bytes());
        w.extend_from_slice(&(ch * 2).to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data.to_le_bytes());
        for i in 0..frames {
            let s = ((i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * 12_000.0) as i16;
            for _ in 0..ch {
                w.extend_from_slice(&s.to_le_bytes());
            }
        }
        w
    }

    /// A reader that hands out a few bytes at a time, like a network stream.
    struct Trickle(Cursor<Vec<u8>>);
    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = buf.len().min(1000);
            self.0.read(&mut buf[..n])
        }
    }

    #[test]
    fn decodes_to_mono_at_the_net_rate() {
        let mut pcm = Vec::new();
        let r = decode(Trickle(Cursor::new(wav(1.0))), None, &AtomicBool::new(false), |b| pcm.extend_from_slice(b));
        assert_eq!(r, Err("the stream ended".into()));
        // one second at NET_RATE, 2 bytes a sample (the resampler holds back a little)
        let secs = pcm.len() as f32 / (NET_RATE as f32 * 2.0);
        assert!((0.9..=1.01).contains(&secs), "{secs} s");
        let loud = pcm.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b).unsigned_abs()).max().unwrap();
        assert!((10_000..=13_000).contains(&loud), "{loud}");
    }

    #[test]
    fn stops_when_asked_and_refuses_what_is_not_audio() {
        let stop = AtomicBool::new(true);
        assert_eq!(decode(Cursor::new(wav(0.5)), Some("MP3"), &stop, |_| panic!("stopped")), Ok(()));
        let e = decode(Cursor::new(b"<html>not audio</html>".to_vec()), None, &AtomicBool::new(false), |_| {});
        assert!(e.unwrap_err().starts_with("not a stream"));
    }

    /// A real station end to end, through the directory and the hub:
    /// `cargo test -p atlas-server -- --ignored live_station_plays`.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn live_station_plays() {
        use atlas_core::audio::{default_tuning, AudioChoice};
        use atlas_core::platform::Os;

        // Radio Romania International (MP3), or another station by uuid
        let net = crate::net::NetService::new(true);
        let uuid = std::env::var("SWATLAS_LIVE_UUID").unwrap_or("9644d860-0601-11e8-ae97-52543be04c81".into());
        let stream = net.resolve(&uuid).await.unwrap();
        let choice =
            AudioChoice { enabled: false, device: String::new(), rate: 16_000, tuning: default_tuning(Os::CURRENT) };
        let hub = AudioHub::new(choice);
        hub.tune_net(stream);
        let mut sub = hub.subscribe().unwrap();
        let mut pcm = Vec::new();
        let t0 = std::time::Instant::now();
        while t0.elapsed() < Duration::from_secs(5) {
            match sub.next_block().await {
                Some(b) => pcm.extend_from_slice(&b),
                None => panic!("the stream ended: {:?}", hub.net_now()),
            }
        }
        let secs = pcm.len() as f32 / (NET_RATE as f32 * 2.0);
        assert!(secs > 2.5, "{secs} s in 5 s");
        assert!(hub.net_playing().is_some());
        let loud = pcm.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b).unsigned_abs()).max().unwrap();
        assert!(loud > 1000, "silent: {loud}");
        drop(sub);
        hub.shutdown().await;
    }

    #[test]
    fn chunks_read_across_messages() {
        let (tx, rx) = mpsc::channel(4);
        tx.try_send(Bytes::from_static(b"abc")).unwrap();
        tx.try_send(Bytes::from_static(b"defg")).unwrap();
        drop(tx);
        let mut s = String::new();
        Chunks::new(rx).read_to_string(&mut s).unwrap();
        assert_eq!(s, "abcdefg");
    }
}
