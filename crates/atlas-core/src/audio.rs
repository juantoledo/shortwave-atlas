//! Rig audio setup: what the settings page may change, how ffmpeg captures on each OS
//! (ALSA, DirectShow, AVFoundation), sound card discovery parsing, choice validation, and
//! plain-language diagnosis of capture problems.

use std::time::Duration;

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

/// The part of `[audio]` the settings page may change. Like `RigChoice`, it has no
/// executable path, so no client can make the core run an arbitrary program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioChoice {
    /// Capture the rig's audio and offer Listen to every client.
    pub enabled: bool,
    /// ALSA capture device, e.g. `plughw:CARD=CODEC,DEV=0`.
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
    /// ffmpeg's input packet queue (`-thread_queue_size`); 0 = ffmpeg's default. ffmpeg says
    /// "Thread message queue blocking" when it is too small.
    pub input_queue: u32,
    /// Clock correction: samples per second aresample may stretch or pad to follow the
    /// capture timestamps (`async`); 0 = off.
    pub drift_correction: u32,
    /// Windows: DirectShow capture buffer in milliseconds (`-audio_buffer_size`).
    pub capture_buffer_ms: u32,
    /// Linux: the rate asked of the card (plughw converts; the rig codec is 48 kHz).
    pub capture_rate: u32,
    /// The listener's cushion in milliseconds, rebuilt after an underrun.
    pub cushion_ms: u32,
    /// Most audio, in milliseconds, queued ahead of the listener's playhead; later blocks are
    /// dropped, so latency cannot grow.
    pub max_ahead_ms: u32,
}

/// The audio path's defaults on `os`. AVFoundation can deliver fewer samples than its
/// declared rate (the stream drains and cuts out about once a second); its timestamps are
/// real time, so macOS follows them (up to ~10 % at 48 kHz). ALSA and DirectShow keep time.
pub fn default_tuning(os: Os) -> AudioTuning {
    AudioTuning {
        block_ms: 20,
        queue_ms: 1_000,
        input_queue: 0,
        drift_correction: match os {
            Os::Linux | Os::Windows => 0,
            Os::Macos => 5_000,
        },
        // DirectShow buffers 500 ms by default; 50 ms keeps the latency of the ALSA path
        capture_buffer_ms: 50,
        capture_rate: 48_000,
        cushion_ms: 120,
        max_ahead_ms: 500,
    }
}

/// A capture-capable sound card device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SoundCard {
    /// Name to use in the config. Linux: an ALSA `plughw` name by card id (converts any
    /// rate and channel count, survives re-plugging in another order). Windows and macOS:
    /// the device's name as DirectShow / AVFoundation list it.
    pub device: String,
    /// What to show: the card name and the PCM name.
    pub label: String,
    pub card_id: String,
    /// A rig's sound card: the TI/Burr-Brown USB audio codec built into Yaesu, Icom and
    /// Kenwood rigs, or a USB interface like the SignaLink or Digirig (see `is_rig_codec`).
    pub rig_codec: bool,
    /// ffmpeg can open it by this name (see `device_name_ok`).
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

/// Audio capture devices from `ffmpeg -list_devices true -f dshow -i dummy` (stderr).
/// Both layouts: ffmpeg >= 4.4 (`"Name" (audio)`) and older (a "DirectShow audio
/// devices" header, then quoted names).
pub fn parse_dshow_devices(stderr: &str) -> Vec<SoundCard> {
    let mut audio_section = false;
    let mut names: Vec<String> = vec![];
    for line in stderr.lines() {
        let body = line.split_once("] ").map_or(line, |(_, b)| b).trim();
        if body.starts_with("DirectShow audio devices") {
            audio_section = true;
            continue;
        }
        if body.starts_with("DirectShow video devices") {
            audio_section = false;
            continue;
        }
        let Some(quoted) = body.strip_prefix('"') else { continue };
        let Some((name, rest)) = quoted.split_once('"') else { continue };
        let kind = rest.trim();
        let audio = if kind.is_empty() { audio_section } else { kind.contains("audio") };
        if audio && !name.is_empty() && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    named_cards(Os::Windows, names)
}

/// Audio capture devices from `ffmpeg -f avfoundation -list_devices true -i ""` (stderr):
/// `[AVFoundation indev @ 0x..] [1] USB AUDIO  CODEC` under "AVFoundation audio devices:".
pub fn parse_avfoundation_devices(stderr: &str) -> Vec<SoundCard> {
    let mut audio_section = false;
    let mut names: Vec<String> = vec![];
    for line in stderr.lines() {
        let body = line.split_once("] ").map_or(line, |(_, b)| b).trim();
        if body.starts_with("AVFoundation audio devices") {
            audio_section = true;
        } else if body.starts_with("AVFoundation video devices") {
            audio_section = false;
        } else if audio_section {
            if let Some((idx, name)) = body.strip_prefix('[').and_then(|b| b.split_once("] ")) {
                if idx.parse::<u32>().is_ok() && !name.trim().is_empty() {
                    names.push(name.trim().to_string());
                }
            }
        }
    }
    named_cards(Os::Macos, names)
}

/// Cards known by name on `os` (Windows, macOS), rig codecs first.
fn named_cards(os: Os, names: Vec<String>) -> Vec<SoundCard> {
    let mut out: Vec<SoundCard> = names
        .into_iter()
        .map(|n| SoundCard {
            label: squash(&n),
            card_id: n.clone(),
            rig_codec: is_rig_codec(&n),
            usable: device_name_ok(os, &n),
            device: n,
        })
        .collect();
    out.sort_by_key(|c| !c.rig_codec);
    out
}

/// ffmpeg's input options for capturing `device` on `os`. The output side (mono,
/// `rate`, s16le on stdout) is the same everywhere.
pub fn capture_input_args(os: Os, device: &str, t: &AudioTuning) -> Vec<String> {
    let (format, input) = match os {
        // plughw converts, so asking for the codec's native rate in stereo always works
        Os::Linux => (vec!["-f", "alsa", "-ac", "2", "-ar"], device.to_string()),
        Os::Windows => (vec!["-f", "dshow", "-audio_buffer_size"], format!("audio={device}")),
        Os::Macos => (vec!["-f", "avfoundation"], format!(":{device}")),
    };
    let mut args: Vec<String> = format.into_iter().map(String::from).collect();
    match os {
        Os::Linux => args.push(t.capture_rate.to_string()),
        Os::Windows => args.push(t.capture_buffer_ms.to_string()),
        Os::Macos => {}
    }
    if t.input_queue > 0 {
        args.extend(["-thread_queue_size".into(), t.input_queue.to_string()]);
    }
    args.extend(["-i".into(), input]);
    args
}

/// ffmpeg's filter options, between the input and the output side: with clock correction
/// on, aresample stretches or pads to the capture timestamps (`first_pts=0` ignores the
/// host-clock start).
pub fn capture_filter_args(t: &AudioTuning) -> Vec<String> {
    match t.drift_correction {
        0 => vec![],
        n => vec!["-af".into(), format!("aresample=async={n}:first_pts=0")],
    }
}

/// A line for the ffmpeg log when `bytes` of s16le mono over `elapsed` is more than 2 %
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

/// A device name ffmpeg can take as one argument after `-i`. On Linux it is limited to ALSA
/// name characters. Elsewhere it is a device name: no `:` or `"`, which DirectShow /
/// AVFoundation would read as a second device.
pub fn device_name_ok(os: Os, d: &str) -> bool {
    let chars_ok = match os {
        Os::Linux => d.chars().all(|ch| ch.is_ascii_alphanumeric() || "_:=,.-".contains(ch)),
        Os::Windows | Os::Macos => d.chars().all(|ch| !ch.is_control() && ch != ':' && ch != '"'),
    };
    !d.is_empty() && d.len() <= 128 && !d.starts_with('-') && d.trim() == d && chars_ok
}

/// Check a choice before it reaches ffmpeg.
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
    range("The input queue", t.input_queue, 0, 4_096, " packets")?;
    range("The clock correction", t.drift_correction, 0, 20_000, " samples/s")?;
    range("The capture buffer", t.capture_buffer_ms, 10, 1_000, " ms")?;
    range("The capture rate", t.capture_rate, 8_000, 192_000, " Hz")?;
    range("The listener cushion", t.cushion_ms, 20, 2_000, " ms")?;
    // above the cushion plus a block, or every block after an underrun would be dropped
    range("The listener maximum", t.max_ahead_ms, t.cushion_ms + t.block_ms + 1, 5_000, " ms")
}

/// State of the audio capture (ffmpeg).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioStatus {
    pub enabled: bool,
    /// ffmpeg is delivering audio.
    pub running: bool,
    pub listeners: u32,
    /// Why ffmpeg stopped last time (e.g. "cannot open audio device ... (Device or resource busy)").
    pub last_error: Option<String>,
    /// ffmpeg could not be started at all.
    pub spawn_error: Option<String>,
    /// Last lines ffmpeg printed (stderr).
    pub log: Vec<String>,
}

/// A likely cause of a capture problem, explained in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AudioHint {
    /// ffmpeg is not installed (or `audio.ffmpeg` points nowhere).
    FfmpegMissing,
    /// Another program (WSJT-X, fldigi, arecord) has the sound card open.
    DeviceBusy,
    /// The sound card is not there (rig unplugged, or the wrong card).
    DeviceMissing,
    /// This user may not open the sound card (`audio` group).
    PermissionDenied,
    /// macOS: SW Atlas is not allowed to use the microphone (Privacy & Security).
    MicrophoneDenied,
}

/// Hints from the ffmpeg status. The ALSA messages are ffmpeg 6.1's, e.g.
/// `cannot open audio device plughw:CARD=CODEC,DEV=0 (Device or resource busy)`; the
/// DirectShow and AVFoundation ones are still to be confirmed on real machines.
pub fn diagnose_audio(os: Os, s: &AudioStatus) -> Vec<AudioHint> {
    if s.spawn_error.is_some() {
        return vec![AudioHint::FfmpegMissing];
    }
    let Some(e) = &s.last_error else { return vec![] };
    match os {
        Os::Linux => {}
        Os::Windows => {
            return if e.contains("Could not find audio only device") || e.contains("Could not enumerate") {
                vec![AudioHint::DeviceMissing]
            } else if e.contains("Could not run graph") || e.contains("Could not set audio options") {
                vec![AudioHint::DeviceBusy]
            } else {
                vec![]
            };
        }
        Os::Macos => {
            let lower = e.to_lowercase();
            return if lower.contains("not authorized") || lower.contains("not permitted") || lower.contains("denied") {
                vec![AudioHint::MicrophoneDenied]
            } else if lower.contains("device not found") || lower.contains("invalid audio device") {
                vec![AudioHint::DeviceMissing]
            } else {
                vec![]
            };
        }
    }
    if e.contains("Device or resource busy") {
        vec![AudioHint::DeviceBusy]
    } else if e.contains("Permission denied") {
        vec![AudioHint::PermissionDenied]
    } else if e.contains("No such device") || e.contains("No such file") || e.contains("Cannot get card index") {
        vec![AudioHint::DeviceMissing]
    } else {
        vec![]
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
        assert_eq!(default_tuning(Os::Macos).drift_correction, 5_000);
        assert_eq!(default_tuning(Os::Linux).drift_correction, 0);
        let d = default_tuning(Os::Linux);
        let bad = |t: AudioTuning| validate_tuning(&t).is_err();
        assert!(bad(AudioTuning { block_ms: 5, ..d }));
        assert!(bad(AudioTuning { block_ms: 101, ..d }));
        assert!(bad(AudioTuning { queue_ms: 30, ..d }), "less than two blocks");
        assert!(bad(AudioTuning { input_queue: 5_000, ..d }));
        assert!(bad(AudioTuning { drift_correction: 20_001, ..d }));
        assert!(bad(AudioTuning { capture_buffer_ms: 5, ..d }));
        assert!(bad(AudioTuning { capture_rate: 4_000, ..d }));
        assert!(bad(AudioTuning { cushion_ms: 10, ..d }));
        assert!(bad(AudioTuning { cushion_ms: 490, ..d }), "no room above the cushion");
        assert!(bad(AudioTuning { max_ahead_ms: 6_000, ..d }));
        let wide = AudioTuning {
            block_ms: 100,
            queue_ms: 5_000,
            input_queue: 4_096,
            cushion_ms: 400,
            max_ahead_ms: 1_000,
            ..d
        };
        assert!(validate_tuning(&wide).is_ok());
        let e = validate_audio(
            Os::Linux,
            &AudioChoice { tuning: AudioTuning { cushion_ms: 10, ..d }, ..choice(Os::Linux, "default") },
        );
        assert_eq!(e.unwrap_err(), "The listener cushion must be between 20 and 2000 ms");
    }

    // ffmpeg 6/7 on Windows (one camera, the onboard mic, an FTDX10)
    const DSHOW: &str = r#"[dshow @ 0000020b8e2c8a00] "Integrated Camera" (video)
[dshow @ 0000020b8e2c8a00]   Alternative name "@device_pnp_\\?\usb#vid_04f2&pid_b6d9&mi_00"
[dshow @ 0000020b8e2c8a00] "Microphone Array (Realtek(R) Audio)" (audio)
[dshow @ 0000020b8e2c8a00]   Alternative name "@device_cm_{33D9A762-90C8-11D0-BD43-00A0C911CE86}\wave_{1}"
[dshow @ 0000020b8e2c8a00] "Microphone (USB AUDIO  CODEC)" (audio)
[dshow @ 0000020b8e2c8a00]   Alternative name "@device_cm_{33D9A762-90C8-11D0-BD43-00A0C911CE86}\wave_{2}"
dummy: Immediate exit requested
"#;
    // ffmpeg < 4.4 layout
    const DSHOW_OLD: &str = r#"[dshow @ 0000000000a1] DirectShow video devices (some may be both video and audio devices)
[dshow @ 0000000000a1]  "Integrated Camera"
[dshow @ 0000000000a1] DirectShow audio devices
[dshow @ 0000000000a1]  "Line (2- USB AUDIO  CODEC)"
"#;
    const AVF: &str = "[AVFoundation indev @ 0x7fc1] AVFoundation video devices:
[AVFoundation indev @ 0x7fc1] [0] FaceTime HD Camera
[AVFoundation indev @ 0x7fc1] [1] Capture screen 0
[AVFoundation indev @ 0x7fc1] AVFoundation audio devices:
[AVFoundation indev @ 0x7fc1] [0] MacBook Pro Microphone
[AVFoundation indev @ 0x7fc1] [1] USB AUDIO  CODEC
: Input/output error
";

    #[test]
    fn windows_and_macos_device_lists() {
        let w = parse_dshow_devices(DSHOW);
        let names: Vec<_> = w.iter().map(|c| c.device.as_str()).collect();
        assert_eq!(names, ["Microphone (USB AUDIO  CODEC)", "Microphone Array (Realtek(R) Audio)"]);
        assert!(w[0].rig_codec && !w[1].rig_codec);
        assert_eq!(w[0].label, "Microphone (USB AUDIO CODEC)");
        let old = parse_dshow_devices(DSHOW_OLD);
        assert_eq!(old.len(), 1);
        assert_eq!(old[0].device, "Line (2- USB AUDIO  CODEC)");
        let m = parse_avfoundation_devices(AVF);
        let names: Vec<_> = m.iter().map(|c| c.device.as_str()).collect();
        assert_eq!(names, ["USB AUDIO  CODEC", "MacBook Pro Microphone"]);
        assert!(parse_dshow_devices("").is_empty() && parse_avfoundation_devices("garbage").is_empty());
    }

    #[test]
    fn capture_args_per_os() {
        let t = default_tuning;
        assert_eq!(
            capture_input_args(Os::Linux, "plughw:CARD=CODEC,DEV=0", &t(Os::Linux)).join(" "),
            "-f alsa -ac 2 -ar 48000 -i plughw:CARD=CODEC,DEV=0"
        );
        let w = capture_input_args(Os::Windows, "Microphone (USB AUDIO  CODEC)", &t(Os::Windows));
        assert_eq!(w, ["-f", "dshow", "-audio_buffer_size", "50", "-i", "audio=Microphone (USB AUDIO  CODEC)"]);
        assert_eq!(
            capture_input_args(Os::Macos, "USB AUDIO  CODEC", &t(Os::Macos)),
            ["-f", "avfoundation", "-i", ":USB AUDIO  CODEC"]
        );
        assert!(capture_filter_args(&t(Os::Linux)).is_empty() && capture_filter_args(&t(Os::Windows)).is_empty());
        assert_eq!(capture_filter_args(&t(Os::Macos)), ["-af", "aresample=async=5000:first_pts=0"]);
    }

    #[test]
    fn capture_args_follow_the_tuning() {
        let mac = AudioTuning { input_queue: 512, drift_correction: 0, ..default_tuning(Os::Macos) };
        assert_eq!(
            capture_input_args(Os::Macos, "USB AUDIO  CODEC", &mac),
            ["-f", "avfoundation", "-thread_queue_size", "512", "-i", ":USB AUDIO  CODEC"]
        );
        assert!(capture_filter_args(&mac).is_empty());
        let lin = AudioTuning { capture_rate: 44_100, drift_correction: 2_000, ..default_tuning(Os::Linux) };
        assert_eq!(capture_input_args(Os::Linux, "hw:1", &lin).join(" "), "-f alsa -ac 2 -ar 44100 -i hw:1");
        assert_eq!(capture_filter_args(&lin), ["-af", "aresample=async=2000:first_pts=0"]);
        let win = AudioTuning { capture_buffer_ms: 200, input_queue: 64, ..default_tuning(Os::Windows) };
        assert_eq!(
            capture_input_args(Os::Windows, "Line", &win).join(" "),
            "-f dshow -audio_buffer_size 200 -thread_queue_size 64 -i audio=Line"
        );
    }

    #[test]
    fn interfaces_marked_and_odd_names_flagged() {
        // a Digirig (C-Media CM108) and a name with a colon ffmpeg cannot be given
        let dshow = r#"[dshow @ 01] "Microphone (Realtek(R) Audio)" (audio)
[dshow @ 01] "Microphone (USB PnP Sound Device)" (audio)
[dshow @ 01] "Line 1: Virtual Cable" (audio)
"#;
        let w = parse_dshow_devices(dshow);
        assert_eq!(w[0].device, "Microphone (USB PnP Sound Device)");
        assert!(w[0].rig_codec && w[0].usable);
        let odd = w.iter().find(|c| c.device.starts_with("Line 1")).unwrap();
        assert!(!odd.rig_codec && !odd.usable);
        let avf = "[AVFoundation indev @ 0x1] AVFoundation audio devices:
[AVFoundation indev @ 0x1] [0] MacBook Pro Microphone
[AVFoundation indev @ 0x1] [1] USB Audio Device
";
        let m = parse_avfoundation_devices(avf);
        assert_eq!(m[0].device, "USB Audio Device");
        assert!(m[0].rig_codec && m.iter().all(|c| c.usable));
        assert!(parse_asound(CARDS, PCM).iter().all(|c| c.usable));
    }

    #[test]
    fn delivered_rate_drift() {
        let s = |n: u64| Duration::from_secs(n);
        assert_eq!(rate_drift(320_000, s(10), 16_000), None);
        assert_eq!(rate_drift(323_000, s(10), 16_000), None); // within 2 %
                                                              // a 44.1 kHz device labelled 48 kHz: 8 % short
        assert_eq!(rate_drift(294_000, s(10), 16_000).as_deref(), Some("delivering 14700 Hz of 16000 Hz"));
        assert_eq!(rate_drift(336_000, s(10), 16_000).as_deref(), Some("delivering 16800 Hz of 16000 Hz"));
        assert_eq!(rate_drift(1_000, Duration::ZERO, 16_000), None);
    }

    #[test]
    fn default_devices_are_valid_on_their_os() {
        for os in [Os::Linux, Os::Windows, Os::Macos] {
            assert!(validate_audio(os, &choice(os, default_device(os))).is_ok(), "{os:?}");
        }
        // and they are the names the device lists report for the codec
        assert_eq!(parse_dshow_devices(DSHOW)[0].device, default_device(Os::Windows));
        assert_eq!(parse_avfoundation_devices(AVF)[0].device, default_device(Os::Macos));
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

    #[test]
    fn windows_and_macos_hints() {
        let st = |e: &str| AudioStatus { last_error: Some(e.into()), ..Default::default() };
        let gone = "[dshow @ 0000020b] Could not find audio only device with name [Microphone (USB AUDIO  CODEC)] among source devices of type audio.";
        assert_eq!(diagnose_audio(Os::Windows, &st(gone)), [AudioHint::DeviceMissing]);
        let busy =
            "[dshow @ 0000020b] Could not run graph (sometimes caused by a device already in use by other application)";
        assert_eq!(diagnose_audio(Os::Windows, &st(busy)), [AudioHint::DeviceBusy]);
        let denied =
            "[AVFoundation indev @ 0x7f] Failed to create AV capture input device: The operation is not authorized";
        assert_eq!(diagnose_audio(Os::Macos, &st(denied)), [AudioHint::MicrophoneDenied]);
        let missing = "[AVFoundation indev @ 0x7f] Audio device not found";
        assert_eq!(diagnose_audio(Os::Macos, &st(missing)), [AudioHint::DeviceMissing]);
        // an ALSA message means nothing to the other OSes
        let alsa = "cannot open audio device plughw:CARD=CODEC,DEV=0 (Device or resource busy)";
        assert!(diagnose_audio(Os::Windows, &st(alsa)).is_empty());
    }

    #[test]
    fn hints_from_real_ffmpeg_errors() {
        let st = |e: &str| AudioStatus { last_error: Some(e.into()), ..Default::default() };
        let busy = "[alsa @ 0x63ac96d6cfc0] cannot open audio device plughw:CARD=CODEC,DEV=0 (Device or resource busy)";
        let gone = "[alsa @ 0x62f2afaa3fc0] cannot open audio device plughw:CARD=NOPE,DEV=0 (No such device)";
        let hw9 = "[alsa @ 0x59db4b384fc0] cannot open audio device hw:9,0 (No such file or directory)";
        let perm = "[alsa @ 0x1] cannot open audio device plughw:CARD=CODEC,DEV=0 (Permission denied)";
        assert_eq!(diagnose_audio(Os::Linux, &st(busy)), [AudioHint::DeviceBusy]);
        assert_eq!(diagnose_audio(Os::Linux, &st(gone)), [AudioHint::DeviceMissing]);
        assert_eq!(diagnose_audio(Os::Linux, &st(hw9)), [AudioHint::DeviceMissing]);
        assert_eq!(diagnose_audio(Os::Linux, &st(perm)), [AudioHint::PermissionDenied]);
        assert!(diagnose_audio(Os::Linux, &st("ffmpeg exited: signal 9")).is_empty());
        assert!(diagnose_audio(Os::Linux, &AudioStatus::default()).is_empty());
        let missing =
            AudioStatus { spawn_error: Some("No such file or directory (os error 2)".into()), ..Default::default() };
        assert_eq!(diagnose_audio(Os::Linux, &missing), [AudioHint::FfmpegMissing]);
    }
}
