//! `Atlas`: the running app (rig + stations + QTH) and the one dispatcher for every UI
//! `Call`, used by both the Tauri shell and the WebSocket.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_core::api::{ApiError, AudioDiagnostics, AudioSettings, Call, ErrorKind, Info, Qth, RigSettings};
use atlas_core::audio::{diagnose_audio, validate_audio, AudioChoice};
use atlas_core::platform::Os;
use atlas_core::setup::{validate_choice, RigChoice, RigModel};
use atlas_core::stations::{list, lookup, Catalog, StationSource, LOOKUP_TOLERANCE_HZ};
use atlas_core::utc_minute;
use atlas_rig::{discover, Rig, RigConfig};
use serde_json::Value;
use tokio::sync::OnceCell;

use crate::audio::{ffmpeg_version, sound_cards, AudioHub};
use crate::config::{save_audio, save_rig, AppConfig};

const SAMPLE_STATIONS: &str = include_str!("../../../data/stations.sample.json");

pub struct Atlas {
    pub rig: Rig,
    /// Rig audio, shared by browsers (`GET /api/audio`) and the desktop app.
    pub audio: Arc<AudioHub>,
    /// Config as loaded at startup; the rig and audio parts may since have changed
    /// (see `rig_config` and `audio.choice()`).
    pub config: AppConfig,
    rig_config: RwLock<RigConfig>,
    /// `swatlas.toml`, where the settings page saves the rig choice.
    config_path: Option<PathBuf>,
    stations: Arc<dyn StationSource>,
    qth: Arc<RwLock<Qth>>,
    /// Where a QTH picked in the UI is saved (kept apart from the hand-edited TOML).
    qth_file: Option<PathBuf>,
    models: OnceCell<Vec<RigModel>>,
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError { kind: ErrorKind::Invalid, message: message.into() }
}

impl Atlas {
    /// Load stations and the saved QTH, and start the rig. Needs a tokio runtime.
    pub fn start(
        config: AppConfig,
        config_path: Option<PathBuf>,
        qth_file: Option<PathBuf>,
    ) -> Result<Arc<Self>, String> {
        let json = match &config.stations {
            Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
            None => SAMPLE_STATIONS.to_string(),
        };
        let stations: Arc<dyn StationSource> = Arc::new(Catalog::from_json(&json).map_err(|e| e.to_string())?);
        let saved = qth_file
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Qth>(&s).ok());
        let qth = Arc::new(RwLock::new(saved.unwrap_or_else(|| config.qth.clone())));
        let rig = Rig::from_config(&config.rig, stations.clone(), qth.clone());
        let rig_config = RwLock::new(config.rig.clone());
        let audio = AudioHub::new(&config.audio.ffmpeg, config.audio.choice());
        Ok(Arc::new(Self {
            rig,
            audio,
            config,
            rig_config,
            config_path,
            stations,
            qth,
            qth_file,
            models: OnceCell::new(),
        }))
    }

    pub fn rig_config(&self) -> RigConfig {
        self.rig_config.read().expect("rig config lock").clone()
    }

    /// Hamlib models, read once (an empty list, e.g. rigctl missing, is retried next time).
    async fn models(&self) -> Vec<RigModel> {
        if let Some(m) = self.models.get() {
            return m.clone();
        }
        let m = discover::rig_models(&self.rig_config().rigctld).await;
        if !m.is_empty() {
            let _ = self.models.set(m.clone());
        }
        m
    }

    async fn rig_settings(&self) -> RigSettings {
        let cfg = self.rig_config();
        RigSettings {
            choice: cfg.choice(),
            locked: self.config.locked.clone(),
            config_path: self.config_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            rigctld_version: discover::rigctld_version(&cfg.rigctld).await,
        }
    }

    /// Validate, switch the rig over, and save the choice into `swatlas.toml`.
    async fn apply_rig(&self, choice: RigChoice) -> Result<(), ApiError> {
        let models = if choice.backend == atlas_rig::BackendKind::Spawn { self.models().await } else { vec![] };
        validate_choice(Os::CURRENT, &choice, &models, discover::port_exists).map_err(invalid)?;
        let next = self.rig_config().with_choice(&choice);
        self.rig.reconfigure(&next).await;
        *self.rig_config.write().expect("rig config lock") = next;
        if let Some(p) = &self.config_path {
            save_rig(p, &choice).map_err(|e| ApiError::internal(format!("Connected, but could not save: {e}")))?;
        }
        Ok(())
    }

    async fn audio_settings(&self) -> AudioSettings {
        AudioSettings {
            choice: self.audio.choice(),
            locked: self.config.audio_locked.clone(),
            config_path: self.config_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            ffmpeg_version: ffmpeg_version(&self.config.audio.ffmpeg).await,
        }
    }

    /// Validate, switch the capture over (listeners reconnect), and save the choice.
    fn apply_audio(&self, choice: AudioChoice) -> Result<(), ApiError> {
        validate_audio(Os::CURRENT, &choice).map_err(invalid)?;
        self.audio.reconfigure(choice.clone());
        if let Some(p) = &self.config_path {
            save_audio(p, &choice).map_err(|e| ApiError::internal(format!("Applied, but could not save: {e}")))?;
        }
        Ok(())
    }

    fn audio_diagnostics(&self) -> AudioDiagnostics {
        let status = self.audio.status();
        AudioDiagnostics { hints: diagnose_audio(Os::CURRENT, &status), status }
    }

    pub fn qth(&self) -> Qth {
        self.qth.read().expect("qth lock").clone()
    }

    fn info(&self) -> Info {
        let audio = self.audio.choice();
        Info {
            version: env!("CARGO_PKG_VERSION").to_string(),
            backend: self.rig_config().backend.as_str().to_string(),
            qth: self.qth(),
            lang: self.config.lang.clone(),
            audio: audio.enabled,
            audio_rate: audio.rate,
            configured: self.config_path.as_ref().is_some_and(|p| p.is_file()),
            os: Os::CURRENT,
        }
    }

    fn set_qth(&self, q: Qth) -> Result<(), ApiError> {
        if !(-90.0..=90.0).contains(&q.lat) || !(-180.0..=180.0).contains(&q.lon) {
            return Err(invalid("QTH out of range"));
        }
        if let Some(p) = &self.qth_file {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let json = serde_json::to_string_pretty(&q).expect("Qth serialises");
            if let Err(e) = std::fs::write(p, json) {
                tracing::warn!("could not save QTH to {}: {e}", p.display());
            }
        }
        *self.qth.write().expect("qth lock") = q;
        Ok(())
    }

    /// Run one UI call, from the desktop app or a browser (WebSocket).
    pub async fn call(&self, call: Call) -> Result<Value, ApiError> {
        let utc_min = utc_minute(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
        let json = |v: Result<Value, serde_json::Error>| v.map_err(|e| ApiError::internal(e.to_string()));
        match call {
            Call::Info => json(serde_json::to_value(self.info())),
            Call::State => json(serde_json::to_value(self.rig.state())),
            Call::Rig(cmd) => {
                self.rig.execute(cmd).await?;
                Ok(Value::Null)
            }
            Call::Lookup { freq_hz } => {
                let c = lookup(&*self.stations, freq_hz, utc_min, self.qth().pos(), LOOKUP_TOLERANCE_HZ);
                json(serde_json::to_value(c))
            }
            Call::List => json(serde_json::to_value(list(&*self.stations, utc_min, self.qth().pos()))),
            Call::SetQth(q) => {
                self.set_qth(q)?;
                Ok(Value::Null)
            }
            Call::RigSettings => json(serde_json::to_value(self.rig_settings().await)),
            Call::RigModels => json(serde_json::to_value(self.models().await)),
            Call::SerialPorts => json(serde_json::to_value(discover::serial_ports())),
            Call::ApplyRig(choice) => {
                self.apply_rig(choice).await?;
                Ok(Value::Null)
            }
            Call::DisconnectRig => {
                self.rig.disconnect().await;
                Ok(Value::Null)
            }
            Call::RigDiagnostics => json(serde_json::to_value(self.rig.diagnostics())),
            Call::AudioSettings => json(serde_json::to_value(self.audio_settings().await)),
            Call::SoundCards => json(serde_json::to_value(sound_cards(&self.config.audio.ffmpeg).await)),
            Call::ApplyAudio(choice) => {
                self.apply_audio(choice)?;
                Ok(Value::Null)
            }
            Call::AudioDiagnostics => json(serde_json::to_value(self.audio_diagnostics())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::audio::default_device;
    use atlas_core::rig::{Power, RigCommand};

    #[tokio::test]
    async fn lookup_and_qth_round_trip() {
        let dir = std::env::temp_dir().join(format!("swatlas-test-{}", std::process::id()));
        let qth_file = dir.join("qth.json");
        let atlas = Atlas::start(AppConfig::default(), None, Some(qth_file.clone())).unwrap();

        let c = atlas.call(Call::Lookup { freq_hz: 13_570_000 }).await.unwrap();
        assert_eq!(c[0]["site"]["id"], "greenville");

        let madrid = Qth { name: "Madrid".into(), lat: 40.4, lon: -3.7 };
        atlas.call(Call::SetQth(madrid.clone())).await.unwrap();
        assert_eq!(atlas.qth(), madrid);
        // a new instance picks the saved QTH up
        let again = Atlas::start(AppConfig::default(), None, Some(qth_file)).unwrap();
        assert_eq!(again.qth(), madrid);
        let _ = std::fs::remove_dir_all(dir);

        let bad = Qth { name: "x".into(), lat: 91.0, lon: 0.0 };
        assert!(atlas.call(Call::SetQth(bad)).await.is_err());
    }

    #[tokio::test]
    async fn sim_rig_is_driven_through_calls() {
        let atlas = Atlas::start(AppConfig::default(), None, None).unwrap();
        let mut rx = atlas.rig.subscribe();
        while atlas.rig.state().power != Power::On {
            rx.changed().await.unwrap();
        }
        atlas.call(Call::Rig(RigCommand::SetFreq { hz: 5_025_000 })).await.unwrap();
        while atlas.rig.state().freq_hz != Some(5_025_000) {
            rx.changed().await.unwrap();
        }
        let info = atlas.call(Call::Info).await.unwrap();
        assert_eq!(info["backend"], "sim");
        assert_eq!(info["audio"], false);
    }

    #[tokio::test]
    async fn apply_audio_validates_switches_and_saves() {
        let dir = std::env::temp_dir().join(format!("swatlas-audio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("swatlas.toml");
        let atlas = Atlas::start(AppConfig::default(), Some(path.clone()), None).unwrap();

        let bad = AudioChoice { enabled: true, device: "-f lavfi".into(), rate: 16_000 };
        let e = atlas.call(Call::ApplyAudio(bad)).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Invalid);
        assert!(!path.exists(), "a refused choice must not be saved");

        let codec = AudioChoice { enabled: true, device: default_device(Os::CURRENT).into(), rate: 8_000 };
        atlas.call(Call::ApplyAudio(codec.clone())).await.unwrap();
        let info = atlas.call(Call::Info).await.unwrap();
        assert_eq!((info["audio"].as_bool(), info["audio_rate"].as_u64()), (Some(true), Some(8_000)));
        let settings = atlas.call(Call::AudioSettings).await.unwrap();
        assert_eq!(settings["choice"]["rate"], 8_000);
        let saved: AppConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.audio.choice(), codec);
        let diag = atlas.call(Call::AudioDiagnostics).await.unwrap();
        assert_eq!(diag["status"]["listeners"], 0);
        assert!(atlas.call(Call::SoundCards).await.unwrap().is_array());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn apply_rig_validates_switches_and_saves() {
        use atlas_core::rig::Link;
        use atlas_core::setup::BackendKind;

        let dir = std::env::temp_dir().join(format!("swatlas-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("swatlas.toml");
        let atlas = Atlas::start(AppConfig::default(), Some(path.clone()), None).unwrap();
        assert_eq!(atlas.call(Call::Info).await.unwrap()["configured"], false);

        let bad = RigChoice {
            backend: BackendKind::Spawn,
            host: "127.0.0.1".into(),
            port: 4532,
            model: 1042,
            device: "/dev/nonexistent-swatlas".into(),
            baud: 9600,
        };
        let e = atlas.call(Call::ApplyRig(bad)).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Invalid);
        assert!(!path.exists(), "a refused choice must not be saved");

        // nothing listens on port 1: the external backend comes up down, and it is saved
        let ext = RigChoice {
            backend: BackendKind::External,
            host: "127.0.0.1".into(),
            port: 1024,
            model: 1,
            device: String::new(),
            baud: 0,
        };
        atlas.call(Call::ApplyRig(ext.clone())).await.unwrap();
        let mut rx = atlas.rig.subscribe();
        while atlas.rig.state().link != Link::Down {
            rx.changed().await.unwrap();
        }
        let settings = atlas.call(Call::RigSettings).await.unwrap();
        assert_eq!(settings["choice"]["backend"], "external");
        let saved: AppConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.rig.choice(), ext);
        let info = atlas.call(Call::Info).await.unwrap();
        assert_eq!((info["configured"].as_bool(), info["backend"].as_str()), (Some(true), Some("external")));

        atlas.call(Call::DisconnectRig).await.unwrap();
        while atlas.rig.state().link != Link::Off {
            rx.changed().await.unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
