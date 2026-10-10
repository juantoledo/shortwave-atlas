//! Rig audio setup: what the settings page may change, sound card discovery and matching,
//! the capture format to ask of a card, the conversion to the listeners' mono PCM, choice
//! validation, and plain-language diagnosis of capture problems. The capture itself (cpal:
//! ALSA, WASAPI, CoreAudio) is IO and lives in `atlas-server`.

use std::time::Duration;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::platform::Os;

/// Sample rates offered in the UI (16-bit mono PCM: 16 kHz = 256 kbit/s per listener).
pub const AUDIO_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];

/// The rig codec's device name on `os`: the default `[audio] device`, so turning audio on
/// works without picking a card when the radio is the usual Burr-Brown codec.
pub fn default_device(os: Os) -> &'static str {
    match os {
        Os::Linux => "plughw:CARD=CODEC,DEV=0",
        Os::Windows => "Microphone (USB AUDIO  CODEC)",
        Os::Macos => "USB AUDIO  CODEC",
    }
}

/// The part of `[audio]` the settings page may change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioChoice {
    /// Capture the rig's audio and offer Listen to every client.
    pub enabled: bool,
    /// Capture device: an ALSA PCM name on Linux (`plughw:CARD=CODEC,DEV=0`), the device's
    /// name elsewhere (`USB AUDIO  CODEC`).
    pub device: String,
    /// Sample rate sent to listeners.
    pub rate: u32,
    /// Buffer sizes and timing, from capture to the listener's speakers.
    pub tuning: AudioTuning,
}

/// Buffer sizes and timing of the audio path (Settings → Audio → Advanced). The defaults
/// come from `default_tuning`; `validate_audio` checks the ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioTuning {
    /// Milliseconds of PCM per block sent to listeners.
    pub block_ms: u32,
    /// Milliseconds of blocks queued for a slow listener before the server drops them.
    pub queue_ms: u32,
    /// The rate asked of the card, before conversion to `rate` (the rig codec is 48 kHz).
    /// A card that cannot do it is opened at the nearest rate it can.
    pub capture_rate: u32,
    /// The listener's cushion in milliseconds, rebuilt after an underrun.
    pub cushion_ms: u32,
    /// Most audio, in milliseconds, queued ahead of the listener's playhead; later blocks are
    /// dropped, so latency cannot grow.
    pub max_ahead_ms: u32,
}

/// The audio path's defaults. The same on every OS: the OS audio APIs deliver the card's
/// samples at its rate (ffmpeg's AVFoundation input lost about 12 % of them on macOS).
pub fn default_tuning(_os: Os) -> AudioTuning {
    AudioTuning { block_ms: 20, queue_ms: 1_000, capture_rate: 48_000, cushion_ms: 120, max_ahead_ms: 500 }
}

/// A capture-capable sound card device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SoundCard {
    /// Name to use in the config. Linux: an ALSA `plughw` name by card id (converts any
    /// rate and channel count, survives re-plugging in another order). Windows and macOS:
    /// the device's name as the OS lists it.
    pub device: String,
    /// What to show: the card name and the PCM name.
    pub label: String,
    pub card_id: String,
    /// A rig's sound card: the TI/Burr-Brown USB audio codec built into Yaesu, Icom and
    /// Kenwood rigs, or a USB interface like the SignaLink or Digirig (see `is_rig_codec`).
    pub rig_codec: bool,
    /// The name can be saved as `[audio] device` (see `device_name_ok`).
    pub usable: bool,
}

/// Capture devices from the text of `/proc/asound/cards` and `/proc/asound/pcm`, rig
/// codecs first. Playback-only devices (speakers, HDMI) are dropped.
pub fn parse_asound(cards: &str, pcm: &str) -> Vec<SoundCard> {
    // " 5 [CODEC          ]: USB-Audio - USB AUDIO  CODEC" then an indented long name
    let mut names: Vec<(u32, String, String, String)> = vec![]; // (index, id, name, long name)
    for line in cards.lines() {
        let Some((head, rest)) = line.split_once("]:") else {
            if let Some(last) = names.last_mut() {
                if last.3.is_empty() {
                    last.3 = line.trim().to_string();
                }
            }
            continue;
        };
        let Some((idx, id)) = head.split_once('[') else { continue };
        let Ok(idx) = idx.trim().parse::<u32>() else { continue };
        let name = rest.split_once(" - ").map_or(rest, |(_, n)| n);
        names.push((idx, id.trim().to_string(), squash(name), String::new()));
    }

    let mut out: Vec<(u32, u32, SoundCard)> = vec![];
    // "05-00: USB Audio : USB Audio : playback 1 : capture 1"
    for line in pcm.lines() {
        let mut parts = line.split(" : ");
        let Some((card, dev)) = parts.next().and_then(|p| p.split(':').next()).and_then(|p| p.split_once('-')) else {
            continue;
        };
        let (Ok(card), Ok(dev)) = (card.trim().parse::<u32>(), dev.trim().parse::<u32>()) else { continue };
        let fields: Vec<&str> = parts.collect();
        if !fields.iter().any(|f| f.trim().starts_with("capture")) {
            continue;
        }
        let Some((_, id, name, long)) = names.iter().find(|n| n.0 == card) else { continue };
        let pcm_name = fields.first().map(|s| squash(s)).unwrap_or_default();
        let rig_codec = is_rig_codec(&format!("{name} {long}"));
        out.push((
            card,
            dev,
            SoundCard {
                device: format!("plughw:CARD={id},DEV={dev}"),
                label: if pcm_name.is_empty() { name.clone() } else { format!("{name} · {pcm_name}") },
                card_id: id.clone(),
                rig_codec,
                usable: true, // a plughw name we built
            },
        ));
    }
    out.sort_by_key(|(card, dev, c)| (!c.rig_codec, *card, *dev));
    out.into_iter().map(|(_, _, c)| c).collect()
}

/// A rig's sound card by name: the TI/Burr-Brown codec in rigs and the SignaLink ("USB AUDIO
/// CODEC", "Microphone (2- USB AUDIO  CODEC)"), or a C-Media interface (Digirig, RigBlaster,
/// CM108/CM119 adapters: "USB PnP Sound Device", "USB Audio Device").
fn is_rig_codec(name: &str) -> bool {
    let lower = squash(&name.to_lowercase());
    ["usb audio codec", "burr", "usb pnp sound device", "usb audio device"].iter().any(|n| lower.contains(n))
}

/// Windows and macOS: the capture devices the OS lists, by name, rig codecs first.
pub fn cards_from_names(os: Os, names: Vec<String>) -> Vec<SoundCard> {
    let mut out: Vec<SoundCard> = vec![];
    for n in names {
        if n.is_empty() || out.iter().any(|c| c.device == n) {
            continue;
        }
        out.push(SoundCard {
            label: squash(&n),
            card_id: n.clone(),
            rig_codec: is_rig_codec(&n),
            usable: device_name_ok(os, &n),
            device: n,
        });
    }
    out.sort_by_key(|c| !c.rig_codec);
    out
}

/// Which of `devices` is the configured `wanted`. Linux: ALSA PCM names, compared in the
/// long form (`plughw:CODEC` = `plughw:CARD=CODEC,DEV=0`). Windows and macOS: device names,
/// exactly, else ignoring case, runs of spaces and the "2- " Windows puts in front of a
/// second device of the same model (so a re-plugged codec is still found).
pub fn find_device(os: Os, wanted: &str, devices: &[String]) -> Option<usize> {
    if let Some(i) = devices.iter().position(|d| d == wanted) {
        return Some(i);
    }
    match os {
        Os::Linux => {
            let w = canonical_alsa(wanted);
            devices.iter().position(|d| canonical_alsa(d) == w)
        }
        Os::Windows | Os::Macos => {
            let w = loose_name(wanted);
            devices.iter().position(|d| loose_name(d) == w)
        }
    }
}

/// `plughw:CODEC` -> `plughw:CARD=CODEC,DEV=0`, `hw:1,0` -> `hw:CARD=1,DEV=0`; anything
/// else (`default`, `pipewire`, names already in the long form) unchanged.
fn canonical_alsa(name: &str) -> String {
    let Some((prefix, rest)) = name.split_once(':') else { return name.to_string() };
    if rest.contains('=') {
        return if rest.contains(',') { name.to_string() } else { format!("{prefix}:{rest},DEV=0") };
    }
    let (card, dev) = rest.split_once(',').unwrap_or((rest, "0"));
    match dev.trim().parse::<u32>() {
        Ok(dev) => format!("{prefix}:CARD={},DEV={dev}", card.trim()),
        Err(_) => name.to_string(),
    }
}

/// "Microphone (2- USB AUDIO  CODEC)" -> "microphone (usb audio codec)".
fn loose_name(name: &str) -> String {
    let lower = squash(&name.to_lowercase());
    match lower.split_once('(') {
        Some((head, tail)) => {
            let digits = tail.chars().take_while(|c| c.is_ascii_digit()).count();
            let tail = match tail[digits..].strip_prefix("- ") {
                Some(t) if digits > 0 => t,
                _ => tail,
            };
            format!("{head}({tail}")
        }
        None => lower,
    }
}

/// A sample format a card offers (cpal's, without the ones no rig codec uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleKind {
    F32,
    I16,
    I32,
    U16,
    /// Anything the capture does not convert (24-bit packed, 64-bit, DSD).
    Other,
}

impl SampleKind {
    /// Preference when a card offers several; `None` = cannot be captured.
    fn rank(self) -> Option<u8> {
        match self {
            Self::F32 => Some(0),
            Self::I16 => Some(1),
            Self::I32 => Some(2),
            Self::U16 => Some(3),
            Self::Other => None,
        }
    }
}

/// One of a card's supported input configurations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureOffer {
    pub channels: u16,
    pub min_rate: u32,
    pub max_rate: u32,
    pub format: SampleKind,
}

/// The configuration to open a card with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureFormat {
    pub channels: u16,
    pub rate: u32,
    pub format: SampleKind,
}

/// What to open a card with: `wanted_rate` (the tuning's `capture_rate`) in mono or stereo
/// when the card can; else the card's own default; else its closest rate. `None` when no
/// offer is in a format the capture converts.
pub fn pick_capture_format(
    offers: &[CaptureOffer],
    wanted_rate: u32,
    default: Option<CaptureFormat>,
) -> Option<CaptureFormat> {
    let usable = || offers.iter().filter(|o| o.channels > 0 && o.format.rank().is_some());
    // fewer channels is less to downmix, but more than 2 often means a multichannel mode
    let key = |o: &CaptureOffer| (o.channels > 2, o.channels, o.format.rank());
    if let Some(o) = usable().filter(|o| (o.min_rate..=o.max_rate).contains(&wanted_rate)).min_by_key(|o| key(o)) {
        return Some(CaptureFormat { channels: o.channels, rate: wanted_rate, format: o.format });
    }
    if let Some(d) = default.filter(|d| d.channels > 0 && d.format.rank().is_some()) {
        return Some(d);
    }
    usable().min_by_key(|o| (wanted_rate.clamp(o.min_rate, o.max_rate).abs_diff(wanted_rate), key(o))).map(|o| {
        CaptureFormat { channels: o.channels, rate: wanted_rate.clamp(o.min_rate, o.max_rate), format: o.format }
    })
}

/// Turns the card's interleaved samples into what listeners get: mono (the channels
/// averaged), resampled to the listener rate, 16-bit little-endian PCM.
pub struct Converter {
    channels: usize,
    /// `None` when the card already runs at the listener rate.
    resampler: Option<Fft<f32>>,
    /// Mono samples waiting for a full resampler chunk.
    pending: Vec<f32>,
    out: Vec<f32>,
}

impl Converter {
    pub fn new(channels: u16, card_rate: u32, rate: u32) -> Result<Self, String> {
        if channels == 0 || card_rate == 0 || rate == 0 {
            return Err(format!("cannot convert {channels} channels at {card_rate} Hz to {rate} Hz"));
        }
        let resampler = if card_rate == rate {
            None
        } else {
            // 10 ms chunks: a few ms of delay, and an FFT size every rate pair allows
            let chunk = (card_rate as usize / 100).max(1);
            Some(
                Fft::<f32>::new(card_rate as usize, rate as usize, chunk, 1, FixedSync::Input)
                    .map_err(|e| format!("resampler {card_rate} -> {rate} Hz: {e}"))?,
            )
        };
        let out = vec![0.0; resampler.as_ref().map_or(0, |r| r.output_frames_max())];
        Ok(Self { channels: channels as usize, resampler, pending: vec![], out })
    }

    /// Add `interleaved` frames from the card (an incomplete last frame is dropped) and
    /// append the PCM ready so far to `pcm`.
    pub fn push(&mut self, interleaved: &[f32], pcm: &mut Vec<u8>) {
        let mono = interleaved.chunks_exact(self.channels).map(|f| f.iter().sum::<f32>() / self.channels as f32);
        let Some(r) = self.resampler.as_mut() else {
            mono.for_each(|s| pcm.extend_from_slice(&to_i16(s).to_le_bytes()));
            return;
        };
        self.pending.extend(mono);
        let mut used = 0;
        while self.pending.len() - used >= r.input_frames_next() {
            let n = r.input_frames_next();
            let (Ok(input), Ok(mut output)) = (
                InterleavedSlice::new(&self.pending[used..used + n], 1, n),
                InterleavedSlice::new_mut(&mut self.out, 1, r.output_frames_max()),
            ) else {
                break;
            };
            let Ok((read, written)) = r.process_into_buffer(&input, &mut output, None) else { break };
            used += read;
            self.out[..written].iter().for_each(|&s| pcm.extend_from_slice(&to_i16(s).to_le_bytes()));
            if read == 0 {
                break;
            }
        }
        self.pending.drain(..used);
    }
}

fn to_i16(s: f32) -> i16 {
    (s * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16
}

/// A line for the capture log when `bytes` of s16le mono over `elapsed` is more than 2 %
/// off `rate` (e.g. "delivering 14700 Hz of 16000 Hz"): the listener's buffer would drain
/// or overflow, so the audio cuts out.
pub fn rate_drift(bytes: u64, elapsed: Duration, rate: u32) -> Option<String> {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 || rate == 0 {
        return None;
    }
    let delivered = bytes as f64 / 2.0 / secs;
    ((delivered / rate as f64 - 1.0).abs() > 0.02)
        .then(|| format!("delivering {} Hz of {rate} Hz", delivered.round() as u64))
}

/// Collapse runs of spaces ("USB AUDIO  CODEC" -> "USB AUDIO CODEC").
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A device name that can go in the config. On Linux it is limited to ALSA name
/// characters. Elsewhere it is a device name: no control characters, quotes or colons.
pub fn device_name_ok(os: Os, d: &str) -> bool {
    let chars_ok = match os {
        Os::Linux => d.chars().all(|ch| ch.is_ascii_alphanumeric() || "_:=,.-".contains(ch)),
        Os::Windows | Os::Macos => d.chars().all(|ch| !ch.is_control() && ch != ':' && ch != '"'),
    };
    !d.is_empty() && d.len() <= 128 && !d.starts_with('-') && d.trim() == d && chars_ok
}

/// Check a choice before the capture uses it.
pub fn validate_audio(os: Os, c: &AudioChoice) -> Result<(), String> {
    if !(8_000..=48_000).contains(&c.rate) {
        return Err("The audio rate must be between 8000 and 48000 Hz".into());
    }
    let d = &c.device;
    if d.is_empty() {
        return Err("Choose a sound card".into());
    }
    if !device_name_ok(os, d) {
        return Err(match os {
            Os::Linux => format!("Not an ALSA device name: {d:?}"),
            _ => format!("Not a sound device name: {d:?}"),
        });
    }
    validate_tuning(&c.tuning)
}

fn validate_tuning(t: &AudioTuning) -> Result<(), String> {
    let range = |what: &str, v: u32, lo: u32, hi: u32, unit: &str| {
        if (lo..=hi).contains(&v) {
            Ok(())
        } else {
            Err(format!("{what} must be between {lo} and {hi}{unit}"))
        }
    };
    range("The block size", t.block_ms, 10, 100, " ms")?;
    range("The server queue", t.queue_ms, 2 * t.block_ms, 5_000, " ms")?;
    range("The capture rate", t.capture_rate, 8_000, 192_000, " Hz")?;
    range("The listener cushion", t.cushion_ms, 20, 2_000, " ms")?;
    // above the cushion plus a block, or every block after an underrun would be dropped
    range("The listener maximum", t.max_ahead_ms, t.cushion_ms + t.block_ms + 1, 5_000, " ms")
}

/// Why the capture stopped, as far as the OS audio API says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CaptureFailure {
    /// Another program has the card (exclusively) open.
    DeviceBusy,
    /// No such device, or it went away (rig unplugged or off).
    DeviceMissing,
    /// The OS refused access (Linux `audio` group, Windows/macOS microphone privacy).
    PermissionDenied,
    Other,
}

/// State of the audio capture.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioStatus {
    pub enabled: bool,
    /// The card is delivering audio.
    pub running: bool,
    pub listeners: u32,
    /// Why the capture stopped last time (e.g. "USB AUDIO  CODEC: device is busy").
    pub last_error: Option<String>,
    /// What kind of failure `last_error` is.
    pub failure: Option<CaptureFailure>,
    /// Capturing, but only exact zeros have arrived (macOS gives a denied app silence).
    pub silent: bool,
    /// The capture's last log lines (device opened, format, warnings).
    pub log: Vec<String>,
}

/// A likely cause of a capture problem, explained in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AudioHint {
    /// Another program (WSJT-X, fldigi, arecord) has the sound card open.
    DeviceBusy,
    /// The sound card is not there (rig unplugged, or the wrong card).
    DeviceMissing,
    /// This user may not open the sound card (Linux `audio` group, Windows microphone privacy).
    PermissionDenied,
    /// macOS: Shortwave Atlas is not allowed to use the microphone (Privacy & Security).
    MicrophoneDenied,
}

/// Hints from the capture status.
pub fn diagnose_audio(os: Os, s: &AudioStatus) -> Vec<AudioHint> {
    if s.silent && s.running && os == Os::Macos {
        return vec![AudioHint::MicrophoneDenied];
    }
    match s.failure {
        Some(CaptureFailure::DeviceBusy) => vec![AudioHint::DeviceBusy],
        Some(CaptureFailure::DeviceMissing) => vec![AudioHint::DeviceMissing],
        Some(CaptureFailure::PermissionDenied) if os == Os::Macos => vec![AudioHint::MicrophoneDenied],
        Some(CaptureFailure::PermissionDenied) => vec![AudioHint::PermissionDenied],
        Some(CaptureFailure::Other) | None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // this machine's /proc/asound, with an FTDX10 on card 5
    const CARDS: &str = " 0 [Audigy2        ]: Audigy2 - SB Audigy 5/Rx [SB1550]
                      SB Audigy 5/Rx [SB1550] (rev.0, serial:0x10241102) at 0xe000, irq 24
 1 [Pebbles        ]: USB-Audio - JBL Pebbles
                      Harman Multimedia JBL Pebbles at usb-0000:11:00.3-2, full speed
 2 [NVidia         ]: HDA-Intel - HDA NVidia
                      HDA NVidia at 0xf6080000 irq 114
 3 [Generic        ]: HDA-Intel - HD-Audio Generic
                      HD-Audio Generic at 0xf6b00000 irq 115
 4 [StudioTM       ]: USB-Audio - Microsoft® LifeCam Studio(TM)
                      Microsoft Microsoft® LifeCam Studio(TM) at usb-0000:11:00.4-2, high speed
 5 [CODEC          ]: USB-Audio - USB AUDIO  CODEC
                      BurrBrown from Texas Instruments USB AUDIO  CODEC at usb-0000:11:00.4-1.2, full
";
    const PCM: &str = "00-00: emu10k1 : ADC Capture/Standard PCM Playback : playback 32 : capture 1
00-01: emu10k1 mic : Mic Capture : capture 1
00-02: emu10k1 efx : Multichannel Capture : capture 1
00-03: emu10k1 : Multichannel Playback : playback 1
01-00: USB Audio : USB Audio : playback 1
02-03: HDMI 0 : HDMI 0 : playback 1
03-00: ALC897 Analog : ALC897 Analog : playback 1 : capture 1
03-01: ALC897 Digital : ALC897 Digital : playback 1
03-02: ALC897 Alt Analog : ALC897 Alt Analog : capture 1
04-00: USB Audio : USB Audio : capture 1
05-00: USB Audio : USB Audio : playback 1 : capture 1
";

    #[test]
    fn capture_devices_rig_codec_first() {
        let cards = parse_asound(CARDS, PCM);
        let devs: Vec<_> = cards.iter().map(|c| c.device.as_str()).collect();
        assert_eq!(
            devs,
            [
                "plughw:CARD=CODEC,DEV=0",
                "plughw:CARD=Audigy2,DEV=0",
                "plughw:CARD=Audigy2,DEV=1",
                "plughw:CARD=Audigy2,DEV=2",
                "plughw:CARD=Generic,DEV=0",
                "plughw:CARD=Generic,DEV=2",
                "plughw:CARD=StudioTM,DEV=0",
            ]
        );
        assert_eq!(cards[0].label, "USB AUDIO CODEC · USB Audio");
        assert_eq!(cards[0].card_id, "CODEC");
        assert!(cards[0].rig_codec);
        assert!(cards[1..].iter().all(|c| !c.rig_codec));
        assert_eq!(cards[6].label, "Microsoft® LifeCam Studio(TM) · USB Audio");
    }

    #[test]
    fn empty_or_garbled_proc_gives_nothing() {
        assert!(parse_asound("", "").is_empty());
        assert!(parse_asound("--- no soundcards ---", "").is_empty());
        assert!(parse_asound(CARDS, "garbage : here").is_empty());
    }

    fn choice(os: Os, device: &str) -> AudioChoice {
        AudioChoice { enabled: true, device: device.into(), rate: 16_000, tuning: default_tuning(os) }
    }

    #[test]
    fn validation() {
        let ok = choice(Os::Linux, "plughw:CARD=CODEC,DEV=0");
        assert!(validate_audio(Os::Linux, &ok).is_ok());
        for d in ["default", "pipewire", "hw:1,0", "dsnoop:CARD=CODEC"] {
            assert!(validate_audio(Os::Linux, &AudioChoice { device: d.into(), ..ok.clone() }).is_ok(), "{d}");
        }
        for d in ["", "-i", "-f lavfi", "hw:1;rm -rf /", "a b", "x\"y", &"x".repeat(129)] {
            assert!(validate_audio(Os::Linux, &AudioChoice { device: d.into(), ..ok.clone() }).is_err(), "{d}");
        }
        for r in [0, 7_999, 48_001] {
            assert!(validate_audio(Os::Linux, &AudioChoice { rate: r, ..ok.clone() }).is_err(), "{r}");
        }
    }

    #[test]
    fn tuning_defaults_and_ranges() {
        for os in [Os::Linux, Os::Windows, Os::Macos] {
            assert!(validate_tuning(&default_tuning(os)).is_ok(), "{os:?}");
        }
        let d = default_tuning(Os::Linux);
        let bad = |t: AudioTuning| validate_tuning(&t).is_err();
        assert!(bad(AudioTuning { block_ms: 5, ..d }));
        assert!(bad(AudioTuning { block_ms: 101, ..d }));
        assert!(bad(AudioTuning { queue_ms: 30, ..d }), "less than two blocks");
        assert!(bad(AudioTuning { capture_rate: 4_000, ..d }));
        assert!(bad(AudioTuning { cushion_ms: 10, ..d }));
        assert!(bad(AudioTuning { cushion_ms: 490, ..d }), "no room above the cushion");
        assert!(bad(AudioTuning { max_ahead_ms: 6_000, ..d }));
        let wide = AudioTuning { block_ms: 100, queue_ms: 5_000, cushion_ms: 400, max_ahead_ms: 1_000, ..d };
        assert!(validate_tuning(&wide).is_ok());
        let e = validate_audio(
            Os::Linux,
            &AudioChoice { tuning: AudioTuning { cushion_ms: 10, ..d }, ..choice(Os::Linux, "default") },
        );
        assert_eq!(e.unwrap_err(), "The listener cushion must be between 20 and 2000 ms");
    }

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn windows_and_macos_device_lists() {
        // what WASAPI and CoreAudio list on a laptop with an FTDX10 plugged in
        let w = cards_from_names(
            Os::Windows,
            names(&["Microphone Array (Realtek(R) Audio)", "Microphone (USB AUDIO  CODEC)"]),
        );
        let devs: Vec<_> = w.iter().map(|c| c.device.as_str()).collect();
        assert_eq!(devs, ["Microphone (USB AUDIO  CODEC)", "Microphone Array (Realtek(R) Audio)"]);
        assert!(w[0].rig_codec && !w[1].rig_codec);
        assert_eq!(w[0].label, "Microphone (USB AUDIO CODEC)");
        let m = cards_from_names(Os::Macos, names(&["MacBook Pro Microphone", "USB AUDIO  CODEC", "USB AUDIO  CODEC"]));
        let devs: Vec<_> = m.iter().map(|c| c.device.as_str()).collect();
        assert_eq!(devs, ["USB AUDIO  CODEC", "MacBook Pro Microphone"], "rig codec first, no duplicates");
        assert!(cards_from_names(Os::Macos, names(&[""])).is_empty());
    }

    #[test]
    fn interfaces_marked_and_odd_names_flagged() {
        // a Digirig (C-Media CM108) and a name with a colon the config cannot hold
        let w = cards_from_names(
            Os::Windows,
            names(&["Microphone (Realtek(R) Audio)", "Microphone (USB PnP Sound Device)", "Line 1: Virtual Cable"]),
        );
        assert_eq!(w[0].device, "Microphone (USB PnP Sound Device)");
        assert!(w[0].rig_codec && w[0].usable);
        let odd = w.iter().find(|c| c.device.starts_with("Line 1")).unwrap();
        assert!(!odd.rig_codec && !odd.usable);
        let m = cards_from_names(Os::Macos, names(&["MacBook Pro Microphone", "USB Audio Device"]));
        assert_eq!(m[0].device, "USB Audio Device");
        assert!(m[0].rig_codec && m.iter().all(|c| c.usable));
        assert!(parse_asound(CARDS, PCM).iter().all(|c| c.usable));
    }

    #[test]
    fn finds_the_configured_device() {
        // Linux: cpal lists ALSA PCM ids in the long form
        let alsa =
            names(&["default", "pipewire", "hw:CARD=CODEC,DEV=0", "plughw:CARD=CODEC,DEV=0", "plughw:CARD=1,DEV=0"]);
        assert_eq!(find_device(Os::Linux, "plughw:CARD=CODEC,DEV=0", &alsa), Some(3));
        assert_eq!(find_device(Os::Linux, "plughw:CODEC", &alsa), Some(3));
        assert_eq!(find_device(Os::Linux, "plughw:CARD=CODEC", &alsa), Some(3));
        assert_eq!(find_device(Os::Linux, "plughw:1,0", &alsa), Some(4));
        assert_eq!(find_device(Os::Linux, "pipewire", &alsa), Some(1));
        assert_eq!(find_device(Os::Linux, "plughw:CARD=NOPE,DEV=0", &alsa), None);
        // Windows renames a re-plugged codec "2- USB AUDIO  CODEC"
        let win = names(&["Microphone Array (Realtek(R) Audio)", "Microphone (2- USB AUDIO  CODEC)"]);
        assert_eq!(find_device(Os::Windows, "Microphone (USB AUDIO  CODEC)", &win), Some(1));
        assert_eq!(find_device(Os::Windows, "microphone (usb audio codec)", &win), Some(1));
        assert_eq!(find_device(Os::Windows, "Line (USB AUDIO  CODEC)", &win), None);
        // an exact name wins over a loose one
        let two = names(&["Microphone (2- USB AUDIO  CODEC)", "Microphone (USB AUDIO  CODEC)"]);
        assert_eq!(find_device(Os::Windows, "Microphone (USB AUDIO  CODEC)", &two), Some(1));
        let mac = names(&["MacBook Air Microphone", "USB AUDIO  CODEC"]);
        assert_eq!(find_device(Os::Macos, "USB AUDIO CODEC", &mac), Some(1));
        assert_eq!(find_device(Os::Macos, "USB AUDIO  CODEC", &[]), None);
    }

    #[test]
    fn default_devices_are_valid_and_found() {
        for os in [Os::Linux, Os::Windows, Os::Macos] {
            assert!(validate_audio(os, &choice(os, default_device(os))).is_ok(), "{os:?}");
        }
        // the names WASAPI and CoreAudio give the codec
        let win = names(&["Microphone (USB AUDIO  CODEC)"]);
        assert_eq!(find_device(Os::Windows, default_device(Os::Windows), &win), Some(0));
        assert_eq!(find_device(Os::Macos, default_device(Os::Macos), &names(&["USB AUDIO  CODEC"])), Some(0));
    }

    #[test]
    fn device_names_per_os() {
        let c = |d: &str| choice(Os::Linux, d);
        for os in [Os::Windows, Os::Macos] {
            assert!(validate_audio(os, &c("Microphone (USB AUDIO  CODEC)")).is_ok());
            assert!(validate_audio(os, &c("Micrófono (2- USB AUDIO  CODEC)")).is_ok());
            for bad in ["", "-list_devices", "a:video=Camera", "x\"y", "tab\there", " lead"] {
                assert!(validate_audio(os, &c(bad)).is_err(), "{os:?} {bad:?}");
            }
        }
        // a Windows name is not an ALSA name
        assert!(validate_audio(Os::Linux, &c("Microphone (USB AUDIO  CODEC)")).is_err());
    }

    const fn offer(channels: u16, min_rate: u32, max_rate: u32, format: SampleKind) -> CaptureOffer {
        CaptureOffer { channels, min_rate, max_rate, format }
    }

    #[test]
    fn capture_format_choice() {
        use SampleKind::*;
        // the macOS codec: 2 ch f32 at 44.1 or 48 kHz
        let codec = [offer(2, 44_100, 44_100, F32), offer(2, 48_000, 48_000, F32)];
        assert_eq!(
            pick_capture_format(&codec, 48_000, None),
            Some(CaptureFormat { channels: 2, rate: 48_000, format: F32 })
        );
        // mono and a preferred format win; 8 channels only if nothing else fits
        let many = [offer(8, 8_000, 96_000, F32), offer(2, 8_000, 96_000, I16), offer(1, 8_000, 96_000, I32)];
        assert_eq!(
            pick_capture_format(&many, 48_000, None),
            Some(CaptureFormat { channels: 1, rate: 48_000, format: I32 })
        );
        let stereo = [offer(2, 8_000, 96_000, I16), offer(2, 8_000, 96_000, F32)];
        assert_eq!(pick_capture_format(&stereo, 48_000, None).unwrap().format, F32);
        // the wanted rate is not offered: the card's default, else the closest rate
        let cd = [offer(2, 44_100, 44_100, I16)];
        let def = CaptureFormat { channels: 2, rate: 44_100, format: I16 };
        assert_eq!(pick_capture_format(&cd, 48_000, Some(def)), Some(def));
        assert_eq!(pick_capture_format(&cd, 48_000, None), Some(def));
        let two = [offer(1, 8_000, 16_000, I16), offer(2, 32_000, 44_100, I16)];
        assert_eq!(pick_capture_format(&two, 48_000, None).unwrap().rate, 44_100);
        // nothing the capture can convert
        assert_eq!(pick_capture_format(&[offer(2, 48_000, 48_000, Other)], 48_000, None), None);
        let bad_default = CaptureFormat { channels: 2, rate: 48_000, format: Other };
        assert_eq!(pick_capture_format(&[], 48_000, Some(bad_default)), None);
    }

    fn pcm_to_i16(pcm: &[u8]) -> Vec<i16> {
        pcm.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect()
    }

    /// Feed `secs` of a `hz` sine in stereo at `card_rate`, in uneven callback sizes.
    fn convert_sine(card_rate: u32, rate: u32, hz: f32, secs: f32) -> Vec<i16> {
        let mut c = Converter::new(2, card_rate, rate).unwrap();
        let n = (card_rate as f32 * secs) as usize;
        let frames: Vec<f32> = (0..n)
            .flat_map(|i| {
                let s = 0.5 * (std::f32::consts::TAU * hz * i as f32 / card_rate as f32).sin();
                [s, s]
            })
            .collect();
        let mut pcm = vec![];
        for chunk in frames.chunks(2 * 333) {
            c.push(chunk, &mut pcm);
        }
        pcm_to_i16(&pcm)
    }

    /// Amplitude of `hz` in `s` (one DFT bin).
    fn level(s: &[i16], rate: u32, hz: f32) -> f32 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in s.iter().enumerate() {
            let p = std::f64::consts::TAU * hz as f64 * i as f64 / rate as f64;
            re += v as f64 * p.cos();
            im += v as f64 * p.sin();
        }
        (2.0 * (re * re + im * im).sqrt() / s.len() as f64 / 32_767.0) as f32
    }

    #[test]
    fn converts_to_mono_pcm_at_the_listener_rate() {
        for (card, rate) in [(48_000, 16_000), (44_100, 16_000), (48_000, 8_000), (44_100, 48_000)] {
            let out = convert_sine(card, rate, 1_000.0, 1.0);
            // every sample arrives, less the resampler's few ms still in flight
            let missing = rate as usize - out.len();
            assert!(missing <= rate as usize / 50, "{card} -> {rate}: {} samples", out.len());
            // the tone keeps its level, skipping the start-up transient
            let a = level(&out[rate as usize / 10..], rate, 1_000.0);
            assert!((a - 0.5).abs() < 0.02, "{card} -> {rate}: level {a}");
        }
        // above the new Nyquist frequency is filtered out, not folded back in as a false tone
        let out = convert_sine(48_000, 16_000, 11_000.0, 1.0);
        assert!(level(&out[1_600..], 16_000, 5_000.0) < 0.005);
        assert!(out[1_600..].iter().all(|s| s.unsigned_abs() < 500), "past the start-up transient: below -30 dB");
    }

    #[test]
    fn same_rate_is_a_plain_downmix() {
        let mut c = Converter::new(2, 16_000, 16_000).unwrap();
        let mut pcm = vec![];
        c.push(&[1.0, 0.0, -0.5, -0.5, 2.0, 2.0, 0.25], &mut pcm); // the odd last sample is dropped
        assert_eq!(pcm_to_i16(&pcm), [16_384, -16_384, 32_767]);
        assert!(Converter::new(0, 48_000, 16_000).is_err());
    }

    #[test]
    fn delivered_rate_drift() {
        let s = |n: u64| Duration::from_secs(n);
        assert_eq!(rate_drift(320_000, s(10), 16_000), None);
        // within 2 %
        assert_eq!(rate_drift(323_000, s(10), 16_000), None);
        // a 44.1 kHz device labelled 48 kHz: 8 % short
        assert_eq!(rate_drift(294_000, s(10), 16_000).as_deref(), Some("delivering 14700 Hz of 16000 Hz"));
        assert_eq!(rate_drift(336_000, s(10), 16_000).as_deref(), Some("delivering 16800 Hz of 16000 Hz"));
        assert_eq!(rate_drift(1_000, Duration::ZERO, 16_000), None);
    }

    #[test]
    fn hints_per_os() {
        let st =
            |f: CaptureFailure| AudioStatus { last_error: Some("x".into()), failure: Some(f), ..Default::default() };
        for os in [Os::Linux, Os::Windows, Os::Macos] {
            assert_eq!(diagnose_audio(os, &st(CaptureFailure::DeviceBusy)), [AudioHint::DeviceBusy]);
            assert_eq!(diagnose_audio(os, &st(CaptureFailure::DeviceMissing)), [AudioHint::DeviceMissing]);
            assert!(diagnose_audio(os, &st(CaptureFailure::Other)).is_empty());
            assert!(diagnose_audio(os, &AudioStatus::default()).is_empty());
        }
        assert_eq!(diagnose_audio(Os::Linux, &st(CaptureFailure::PermissionDenied)), [AudioHint::PermissionDenied]);
        assert_eq!(diagnose_audio(Os::Windows, &st(CaptureFailure::PermissionDenied)), [AudioHint::PermissionDenied]);
        assert_eq!(diagnose_audio(Os::Macos, &st(CaptureFailure::PermissionDenied)), [AudioHint::MicrophoneDenied]);
        // macOS hands a denied app silence instead of an error
        let silent = AudioStatus { running: true, silent: true, ..Default::default() };
        assert_eq!(diagnose_audio(Os::Macos, &silent), [AudioHint::MicrophoneDenied]);
        assert!(diagnose_audio(Os::Linux, &silent).is_empty());
        assert!(diagnose_audio(Os::Macos, &AudioStatus { running: false, ..silent }).is_empty());
    }
}
