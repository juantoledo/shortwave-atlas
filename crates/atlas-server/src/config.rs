//! `swatlas.toml` plus environment overrides (the prototype's variable names still work).

use std::net::IpAddr;
use std::path::{Path, PathBuf};

use atlas_core::api::Qth;
use atlas_core::audio::{validate_audio, AudioChoice};
use atlas_core::platform::Os;
use atlas_core::setup::RigChoice;
use atlas_rig::{BackendKind, RigConfig};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, Value};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub rig: RigConfig,
    pub server: ServerConfig,
    pub audio: AudioConfig,
    /// Starting QTH; a QTH picked in the UI is saved separately (see `Atlas`).
    pub qth: Qth,
    /// UI language (`en`, `es`); unset = follow the browser/OS.
    pub lang: Option<String>,
    /// Station data JSON; unset = the bundled sample.
    pub stations: Option<PathBuf>,
    /// `RigChoice` fields forced by environment variables (not part of the file).
    #[serde(skip)]
    pub locked: Vec<String>,
    /// `AudioChoice` fields forced by environment variables.
    #[serde(skip)]
    pub audio_locked: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Desktop app only: also serve the remote browser UI. The headless server always does.
    pub enabled: bool,
    pub bind: String,
    pub port: u16,
    /// `user:password` for HTTP basic auth. Required to bind beyond loopback.
    pub auth: Option<String>,
    /// Built UI (`ui/dist`); unset = look next to the working directory and the binary.
    pub ui_dir: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { enabled: false, bind: "127.0.0.1".into(), port: 8080, auth: None, ui_dir: None }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Capture the rig's audio (ffmpeg) and offer Listen to every client.
    pub enabled: bool,
    /// ALSA device of the rig's USB codec.
    pub device: String,
    /// Sample rate sent to listeners (16-bit mono PCM).
    pub rate: u32,
    /// `ffmpeg` executable. Never settable from the UI (see `AudioChoice`).
    pub ffmpeg: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { enabled: false, device: "plughw:CARD=CODEC,DEV=0".into(), rate: 16_000, ffmpeg: "ffmpeg".into() }
    }
}

impl AudioConfig {
    pub fn choice(&self) -> AudioChoice {
        AudioChoice { enabled: self.enabled, device: self.device.clone(), rate: self.rate }
    }

    pub fn with_choice(&self, c: &AudioChoice) -> Self {
        Self { enabled: c.enabled, device: c.device.clone(), rate: c.rate, ffmpeg: self.ffmpeg.clone() }
    }
}

/// Default config directory: `~/.config/swatlas` on Linux.
pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("swatlas")
}

impl AppConfig {
    /// Load `path` (missing file = defaults), then apply environment overrides.
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).map_err(|e| format!("{}: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        cfg.apply_env(|k| std::env::var(k).ok())?;
        Ok(cfg)
    }

    /// Environment overrides; `get` is `std::env::var` outside tests.
    pub fn apply_env(&mut self, get: impl Fn(&str) -> Option<String>) -> Result<(), String> {
        fn num<T: std::str::FromStr>(k: &str, v: String) -> Result<T, String> {
            v.parse().map_err(|_| format!("{k}: not a number: {v:?}"))
        }
        // which rig fields the environment forces (the settings page warns about them)
        for (var, field) in [
            ("SWATLAS_RIG", "backend"),
            ("RIGCTLD_HOST", "host"),
            ("RIGCTLD_PORT", "port"),
            ("SWATLAS_RIG_MODEL", "model"),
            ("SWATLAS_RIG_DEVICE", "device"),
            ("SWATLAS_RIG_BAUD", "baud"),
        ] {
            if get(var).is_some() && !self.locked.iter().any(|f| f == field) {
                self.locked.push(field.into());
            }
        }
        for (var, fields) in [("AUDIO_DEVICE", &["enabled", "device"][..]), ("AUDIO_RATE", &["rate"][..])] {
            if get(var).is_some() {
                for f in fields {
                    if !self.audio_locked.iter().any(|x| x == f) {
                        self.audio_locked.push(f.to_string());
                    }
                }
            }
        }
        if let Some(v) = get("SWATLAS_RIG") {
            self.rig.backend = match v.as_str() {
                "sim" => BackendKind::Sim,
                "external" => BackendKind::External,
                "spawn" => BackendKind::Spawn,
                _ => return Err(format!("SWATLAS_RIG must be sim, external or spawn, not {v:?}")),
            };
        }
        if let Some(v) = get("RIGCTLD_HOST") {
            self.rig.host = v;
        }
        if let Some(v) = get("RIGCTLD_PORT") {
            self.rig.port = num("RIGCTLD_PORT", v)?;
        }
        if let Some(v) = get("SWATLAS_RIG_MODEL") {
            self.rig.model = num("SWATLAS_RIG_MODEL", v)?;
        }
        if let Some(v) = get("SWATLAS_RIG_DEVICE") {
            self.rig.device = v;
        }
        if let Some(v) = get("SWATLAS_RIG_BAUD") {
            self.rig.baud = num("SWATLAS_RIG_BAUD", v)?;
        }
        if let Some(v) = get("WEB_BIND") {
            self.server.bind = v;
        }
        if let Some(v) = get("WEB_PORT") {
            self.server.port = num("WEB_PORT", v)?;
        }
        if let Some(v) = get("WEB_AUTH") {
            self.server.auth = Some(v).filter(|s| !s.is_empty());
        }
        if let Some(v) = get("AUDIO_DEVICE") {
            self.audio.device = v;
            self.audio.enabled = true;
        }
        if let Some(v) = get("AUDIO_RATE") {
            self.audio.rate = num("AUDIO_RATE", v)?;
        }
        if let Some(v) = get("SWATLAS_LANG") {
            self.lang = Some(v);
        }
        if let Some(v) = get("SWATLAS_STATIONS") {
            self.stations = Some(v.into());
        }
        if let Some(v) = get("SWATLAS_UI_DIR") {
            self.server.ui_dir = Some(v.into());
        }
        Ok(())
    }

    /// Refuse unsafe server settings before anything listens.
    pub fn validate_server(&self) -> Result<(), String> {
        let s = &self.server;
        if let Some(a) = &s.auth {
            if !a.contains(':') {
                return Err("server.auth must be \"user:password\"".into());
            }
        }
        let loopback = s.bind == "localhost" || s.bind.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
        if !loopback && s.auth.is_none() {
            return Err(format!(
                "refusing to listen on {} without a password: set server.auth (or WEB_AUTH=user:password)",
                s.bind
            ));
        }
        if self.audio.enabled {
            validate_audio(Os::CURRENT, &self.audio.choice()).map_err(|e| format!("[audio]: {e}"))?;
        }
        Ok(())
    }
}

/// Write `choice` into the `[rig]` table of the TOML at `path`, in place: comments,
/// formatting, other sections and keys like `rigctld` are kept. Creates the file if needed.
pub fn save_rig(path: &Path, choice: &RigChoice) -> Result<(), String> {
    save_table(
        path,
        "rig",
        [
            ("backend", choice.backend.as_str().into()),
            ("host", choice.host.as_str().into()),
            ("port", i64::from(choice.port).into()),
            ("model", i64::from(choice.model).into()),
            ("device", choice.device.as_str().into()),
            ("baud", i64::from(choice.baud).into()),
        ],
    )
}

/// Write `choice` into the `[audio]` table, the same way as `save_rig` (`ffmpeg` is kept).
pub fn save_audio(path: &Path, choice: &AudioChoice) -> Result<(), String> {
    save_table(
        path,
        "audio",
        [
            ("enabled", choice.enabled.into()),
            ("device", choice.device.as_str().into()),
            ("rate", i64::from(choice.rate).into()),
        ],
    )
}

/// Set `keys` in table `name` of the TOML at `path`, keeping everything else.
fn save_table<const N: usize>(path: &Path, name: &str, keys: [(&str, Value); N]) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
    let table = doc.entry(name).or_insert(Item::Table(Table::new()));
    let Some(table) = table.as_table_mut() else {
        return Err(format!("{}: `{name}` is not a table", path.display()));
    };
    for (key, v) in keys {
        match table.get_mut(key).and_then(Item::as_value_mut) {
            // keep the key's decoration, e.g. a trailing `# comment`
            Some(old) => {
                let decor = old.decor().clone();
                *old = v;
                *old.decor_mut() = decor;
            }
            None => {
                table.insert(key, Item::Value(v));
            }
        }
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    // write next to the target and rename, so a crash never leaves half a config
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, doc.to_string()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn defaults_are_safe() {
        let c = AppConfig::default();
        assert_eq!(c.rig.backend, BackendKind::Sim);
        assert_eq!(c.server.bind, "127.0.0.1");
        assert!(!c.server.enabled && !c.audio.enabled);
        c.validate_server().unwrap();
    }

    #[test]
    fn ftdx10_toml() {
        let c: AppConfig = toml::from_str(
            r#"
            [rig]
            backend = "spawn"
            model = 1042
            device = "/dev/serial/by-id/usb-Silicon_Labs_CP2105-if00-port0"
            baud = 9600
            [audio]
            enabled = true
            "#,
        )
        .unwrap();
        assert_eq!((c.rig.backend, c.rig.model, c.rig.baud, c.rig.port), (BackendKind::Spawn, 1042, 9600, 4532));
        assert_eq!(c.audio.rate, 16_000);
    }

    #[test]
    fn prototype_env_names_still_work() {
        let mut c = AppConfig::default();
        c.apply_env(env(&[
            ("SWATLAS_RIG", "external"),
            ("RIGCTLD_PORT", "4533"),
            ("WEB_BIND", "0.0.0.0"),
            ("WEB_AUTH", "me:secret"),
            ("AUDIO_DEVICE", "hw:1"),
        ]))
        .unwrap();
        assert_eq!((c.rig.backend, c.rig.port), (BackendKind::External, 4533));
        assert_eq!(c.server.auth.as_deref(), Some("me:secret"));
        assert!(c.audio.enabled);
        c.validate_server().unwrap();
    }

    #[test]
    fn bad_env_is_an_error() {
        let mut c = AppConfig::default();
        assert!(c.apply_env(env(&[("WEB_PORT", "eighty")])).is_err());
        assert!(c.apply_env(env(&[("SWATLAS_RIG", "ftdx10")])).is_err());
    }

    #[test]
    fn env_forced_rig_fields_are_locked() {
        let mut c = AppConfig::default();
        c.apply_env(env(&[("SWATLAS_RIG", "spawn"), ("SWATLAS_RIG_BAUD", "9600"), ("WEB_PORT", "8081")])).unwrap();
        assert_eq!(c.locked, ["backend", "baud"]);
        assert!(c.audio_locked.is_empty());
    }

    #[test]
    fn env_forced_audio_fields_are_locked() {
        let mut c = AppConfig::default();
        c.apply_env(env(&[("AUDIO_RATE", "8000")])).unwrap();
        assert_eq!(c.audio_locked, ["rate"]);
        let mut c = AppConfig::default();
        c.apply_env(env(&[("AUDIO_DEVICE", "hw:1"), ("AUDIO_RATE", "8000")])).unwrap();
        assert_eq!(c.audio_locked, ["enabled", "device", "rate"]);
    }

    #[test]
    fn save_audio_keeps_comments_and_the_rig() {
        let path = tmp("audio");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, include_str!("../../../docs/swatlas.example.toml")).unwrap();
        let choice = AudioChoice { enabled: true, device: "plughw:CARD=CODEC,DEV=0".into(), rate: 8_000 };
        save_audio(&path, &choice).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let rate = text.lines().find(|l| l.starts_with("rate =")).unwrap();
        assert!(
            rate.starts_with("rate = 8000 ") && rate.ends_with("# 16-bit mono PCM: 16 kHz = 256 kbit/s per listener"),
            "{rate}"
        );
        assert!(text.contains("model = 1042                  # FTDX10 (`rigctl -l`)"));
        let c: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!(c.audio.choice(), choice);
        assert_eq!((c.rig.model, c.audio.ffmpeg.as_str()), (1042, "ffmpeg"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn bad_audio_is_refused_at_start() {
        let mut c = AppConfig::default();
        c.audio.rate = 4_000;
        c.validate_server().unwrap(); // disabled: not checked
        c.audio.enabled = true;
        assert!(c.validate_server().is_err());
        c.audio.rate = 16_000;
        c.audio.device = "-f lavfi".into();
        assert!(c.validate_server().is_err());
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("swatlas-cfg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("swatlas.toml")
    }

    fn ftdx10() -> RigChoice {
        RigChoice {
            backend: BackendKind::Spawn,
            host: "127.0.0.1".into(),
            port: 4532,
            model: 1042,
            device: "/dev/ttyUSB0".into(),
            baud: 9600,
        }
    }

    #[test]
    fn save_rig_keeps_comments_and_other_sections() {
        let path = tmp("keep");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, include_str!("../../../docs/swatlas.example.toml")).unwrap();
        let mut choice = ftdx10();
        choice.model = 1040;
        choice.baud = 38400;
        save_rig(&path, &choice).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("model = 1040                  # FTDX10 (`rigctl -l`)"), "{text}");
        assert!(text.contains("baud = 38400"));
        assert!(text.contains("# SW Atlas config:"));
        assert!(text.contains("# rigctld = \"rigctld\""));
        let c: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!(c.rig.choice(), choice);
        assert_eq!(c.qth.name, "Santiago, Chile");
        assert_eq!(c.audio.rate, 16_000);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_rig_creates_the_file() {
        let path = tmp("new");
        save_rig(&path, &ftdx10()).unwrap();
        let c: AppConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(c.rig.choice(), ftdx10());
        assert_eq!(c.rig.rigctld, "rigctld");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn open_bind_needs_auth() {
        let mut c = AppConfig::default();
        c.server.bind = "0.0.0.0".into();
        assert!(c.validate_server().is_err());
        c.server.auth = Some("nocolon".into());
        assert!(c.validate_server().is_err());
        c.server.auth = Some("u:p".into());
        assert!(c.validate_server().is_ok());
    }
}
