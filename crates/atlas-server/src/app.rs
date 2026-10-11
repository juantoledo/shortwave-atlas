//! `Atlas`: the running app (rig + stations + QTH) and the one dispatcher for every UI
//! `Call`, used by both the Tauri shell and the WebSocket.

use std::path::PathBuf;
use std::sync::{Arc, RwLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_core::api::{
    ApiError, AudioDiagnostics, AudioSettings, Call, ErrorKind, Info, Qth, RigSettings, UpdatePrefs,
};
use atlas_core::audio::{default_tuning, diagnose_audio, validate_audio, AudioChoice};
use atlas_core::bands::{Dial, ItuRegion};
use atlas_core::lens::{self, Known};
use atlas_core::platform::Os;
use atlas_core::scan::{plan, ScanRequest, ScanStatus};
use atlas_core::setup::{validate_choice, BackendKind, RigChoice, RigModel};
use atlas_core::stations::{CatalogCell, LOOKUP_TOLERANCE_HZ};
use atlas_rig::{discover, Rig, RigConfig, ScanStop};
use serde_json::Value;
use tokio::sync::OnceCell;

use crate::audio::{backend, sound_cards, AudioHub};
use crate::config::{save_audio, save_rig, save_update, AppConfig};
use crate::favorites::FavoriteService;
use crate::scan::{Context, ScanService};
use crate::update::{InstallRefused, UpdateService, Updater};

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
    /// The running catalog, built from the sources.
    stations: Arc<CatalogCell>,
    qth: Arc<RwLock<Qth>>,
    /// Where a QTH picked in the UI is saved (kept apart from the hand-edited TOML).
    qth_file: Option<PathBuf>,
    models: OnceCell<Vec<RigModel>>,
    /// "Update available": checks, and installs on the desktop.
    pub update: Arc<UpdateService>,
    /// Band scans and the logbook.
    pub scan: Arc<ScanService>,
    pub favorites: Arc<FavoriteService>,
    /// Ourselves, for the background install task.
    me: Weak<Atlas>,
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError { kind: ErrorKind::Invalid, message: message.into() }
}

impl Atlas {
    /// Load stations and, from `data_dir`, the saved QTH (`qth.json`), scans (`scans/`) and
    /// favorites (`favorites.json`); then start the rig and the update checks. `None` keeps all of
    /// them in memory (tests). Needs a tokio runtime. `updater` is the desktop shell's, or a
    /// `ManifestUpdater`.
    pub fn start(
        config: AppConfig,
        config_path: Option<PathBuf>,
        data_dir: Option<PathBuf>,
        updater: Arc<dyn Updater>,
    ) -> Result<Arc<Self>, String> {
        let qth_file = data_dir.as_ref().map(|d| d.join("qth.json"));
        if let Some(p) = &config.stations {
            tracing::warn!("`stations = {}` is no longer used; see `eibi_dir`", p.display());
        }
        let eibi = match &config.eibi_dir {
            Some(dir) => crate::sources::load(dir)?,
            None => crate::sources::bundled(),
        };
        let stations = CatalogCell::new(crate::sources::catalog(&eibi));
        let saved = qth_file
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Qth>(&s).ok());
        let qth = Arc::new(RwLock::new(saved.unwrap_or_else(|| config.qth.clone())));
        let rig = Rig::from_config(&config.rig, stations.clone(), qth.clone());
        let rig_config = RwLock::new(config.rig.clone());
        let audio = AudioHub::new(config.audio.choice());
        let update = UpdateService::new(updater, &config.update, config.update_locked.clone());
        update.spawn_checks();
        let scan = ScanService::new(data_dir.as_ref().map(|d| d.join("scans")), stations.clone());
        let favorites = FavoriteService::new(data_dir.as_ref().map(|d| d.join("favorites.json")), stations.clone());
        Ok(Arc::new_cyclic(|me| Self {
            rig,
            audio,
            config,
            rig_config,
            config_path,
            stations,
            qth,
            qth_file,
            models: OnceCell::new(),
            update,
            scan,
            favorites,
            me: me.clone(),
        }))
    }

    /// Stop the audio capture and the rigctld we started, and wait until they are gone, so the
    /// serial port, rigctld's TCP port and the sound card are free when the app is gone.
    pub async fn shutdown(&self) {
        self.audio.shutdown().await;
        self.rig.disconnect().await;
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

    fn audio_settings(&self) -> AudioSettings {
        AudioSettings {
            choice: self.audio.choice(),
            locked: self.config.audio_locked.clone(),
            config_path: self.config_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            backend: backend(),
            defaults: default_tuning(Os::CURRENT),
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

    /// Start downloading and installing the offered update in the background.
    fn install_update(&self) -> Result<(), ApiError> {
        self.update.begin_install().map_err(|e| match e {
            InstallRefused::Manual => {
                invalid("This copy of Shortwave Atlas is updated by hand: download the new version.")
            }
            InstallRefused::NotNow(why) => ApiError { kind: ErrorKind::Conflict, message: why.into() },
        })?;
        let atlas = self.me.upgrade().ok_or_else(|| ApiError::internal("shutting down"))?;
        tokio::spawn(async move {
            let (a, b) = (atlas.clone(), atlas.clone());
            atlas.update.clone().run_install(async move { a.shutdown().await }, async move { b.resume().await }).await;
        });
        Ok(())
    }

    /// Bring the rig back after an install that failed past `shutdown` (audio restarts with
    /// the next listener).
    async fn resume(&self) {
        self.rig.reconfigure(&self.rig_config()).await;
    }

    fn set_update_prefs(&self, p: UpdatePrefs) -> Result<(), ApiError> {
        self.update.set_prefs(p.check, p.channel);
        if let Some(path) = &self.config_path {
            save_update(path, p.check, p.channel)
                .map_err(|e| ApiError::internal(format!("Changed, but could not save: {e}")))?;
        }
        Ok(())
    }

    /// Check the request against the rig's dial, take the rig from the poller and start sweeping.
    async fn start_scan(&self, req: ScanRequest) -> Result<ScanStatus, ApiError> {
        let coverage = self.rig.coverage().await;
        let dial = Dial::new(ItuRegion::of(self.qth().pos()), coverage.clone());
        let band = dial
            .bands
            .iter()
            .find(|b| b.id == req.band_id)
            .ok_or_else(|| invalid("That band is not on this rig's dial"))?;
        let plan = plan(band, req.step_hz, req.dwell_ms, &coverage).map_err(invalid)?;
        let link = self.rig.begin_scan()?;
        let state = self.rig.state();
        let cfg = self.rig_config();
        let rig = match cfg.backend {
            BackendKind::Spawn => format!("spawn, Hamlib model {}", cfg.model),
            b => b.as_str().to_string(),
        };
        let ctx = Context { qth: self.qth(), rig, mode: state.mode, back_hz: state.freq_hz };
        Ok(self.scan.start(link, plan, ctx))
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
            audio_cushion_ms: audio.tuning.cushion_ms,
            audio_max_ahead_ms: audio.tuning.max_ahead_ms,
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
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let json = |v: Result<Value, serde_json::Error>| v.map_err(|e| ApiError::internal(e.to_string()));
        match call {
            Call::Info => json(serde_json::to_value(self.info())),
            Call::State => json(serde_json::to_value(self.rig.state())),
            Call::Rig(cmd) => {
                self.rig.execute(cmd).await?;
                Ok(Value::Null)
            }
            Call::Lookup { freq_hz } => {
                let c = self.stations.get().lookup(freq_hz, now, self.qth().pos(), LOOKUP_TOLERANCE_HZ);
                json(serde_json::to_value(c))
            }
            Call::Search(q) => {
                let cat = self.stations.get();
                let (logbook, signals, favorites) =
                    (self.scan.logbook_for(&cat), self.scan.signals(), self.favorites.list());
                let scan = q.scan.as_deref().and_then(|id| self.scan.summary(id));
                let known = Known { logbook: &logbook, signals: &signals, favorites: &favorites, scan: scan.as_ref() };
                json(serde_json::to_value(lens::search(&cat, &q, now, &known)))
            }
            Call::Overview => json(serde_json::to_value(self.stations.get().overview(now))),
            Call::StationsMeta => json(serde_json::to_value(self.stations.get().meta(now))),
            Call::SetQth(q) => {
                self.set_qth(q)?;
                Ok(Value::Null)
            }
            Call::Dial => {
                let region = ItuRegion::of(self.qth().pos());
                json(serde_json::to_value(Dial::new(region, self.rig.coverage().await)))
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
            Call::AudioSettings => json(serde_json::to_value(self.audio_settings())),
            Call::SoundCards => json(serde_json::to_value(sound_cards().await)),
            Call::ApplyAudio(choice) => {
                self.apply_audio(choice)?;
                Ok(Value::Null)
            }
            Call::AudioDiagnostics => json(serde_json::to_value(self.audio_diagnostics())),
            Call::UpdateStatus => json(serde_json::to_value(self.update.status())),
            Call::CheckUpdate => json(serde_json::to_value(self.update.check_now().await)),
            Call::InstallUpdate => {
                self.install_update()?;
                Ok(Value::Null)
            }
            Call::SetUpdatePrefs(p) => {
                self.set_update_prefs(p)?;
                Ok(Value::Null)
            }
            Call::StartScan(req) => json(serde_json::to_value(self.start_scan(req).await?)),
            Call::StopScan => {
                self.rig.stop_scan(ScanStop::User).await;
                Ok(Value::Null)
            }
            Call::ScanStatus => json(serde_json::to_value(self.scan.status())),
            Call::Scans => json(serde_json::to_value(self.scan.summaries())),
            Call::Scan { id } => match self.scan.scan(&id) {
                Some(s) => json(serde_json::to_value(s)),
                None => Err(invalid("No such scan")),
            },
            Call::DeleteScan { id } => match self.scan.delete(&id) {
                Ok(true) => Ok(Value::Null),
                Ok(false) => Err(invalid("No such scan")),
                Err(e) => Err(ApiError::internal(format!("Could not delete the scan: {e}"))),
            },
            Call::DeleteScans => match self.scan.delete_all() {
                Ok(n) => json(serde_json::to_value(n)),
                Err(e) => Err(ApiError::internal(format!("Could not delete the scans: {e}"))),
            },
            Call::Logbook => json(serde_json::to_value(self.scan.logbook().as_slice())),
            Call::Favorites => json(serde_json::to_value(self.favorites.views())),
            Call::AddFavorite(req) => json(serde_json::to_value(self.favorites.add(&req, now)?)),
            Call::RenameFavorite { id, label } => {
                self.favorites.rename(&id, &label)?;
                Ok(Value::Null)
            }
            Call::DeleteFavorite { id } => {
                self.favorites.delete(&id)?;
                Ok(Value::Null)
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use atlas_core::audio::default_device;
    use atlas_core::rig::{Power, RigCommand};
    use atlas_core::stations::StationQuery;
    use atlas_core::update::{Channel, Packaging, UpdateState};

    use crate::update::fake::FakeUpdater;

    /// The server's updater, finding nothing.
    pub(crate) fn no_updates() -> Arc<dyn Updater> {
        FakeUpdater::new(Packaging::Server, None)
    }

    #[tokio::test]
    async fn lookup_and_qth_round_trip() {
        let dir = std::env::temp_dir().join(format!("swatlas-test-{}", std::process::id()));
        let atlas = Atlas::start(AppConfig::default(), None, Some(dir.clone()), no_updates()).unwrap();

        // the first station in the list is found again by its frequency
        let page = atlas.call(Call::Search(StationQuery { limit: 1, ..Default::default() })).await.unwrap();
        let first = &page["items"][0];
        let c = atlas.call(Call::Lookup { freq_hz: first["freq_hz"].as_u64().unwrap() as u32 }).await.unwrap();
        assert!(c.as_array().unwrap().iter().any(|c| c["station"]["id"] == first["id"]), "{c}");

        let madrid = Qth { name: "Madrid".into(), lat: 40.4, lon: -3.7 };
        atlas.call(Call::SetQth(madrid.clone())).await.unwrap();
        assert_eq!(atlas.qth(), madrid);
        // a new instance picks the saved QTH up
        assert!(dir.join("qth.json").is_file());
        let again = Atlas::start(AppConfig::default(), None, Some(dir.clone()), no_updates()).unwrap();
        assert_eq!(again.qth(), madrid);
        let _ = std::fs::remove_dir_all(dir);

        // the dial follows the QTH's ITU region and the sim's coverage
        let dial = atlas.call(Call::Dial).await.unwrap();
        assert_eq!(dial["region"], "R1");
        let forty = dial["bands"].as_array().unwrap().iter().find(|b| b["id"] == "ham-40m").unwrap();
        assert_eq!(forty["hi_khz"], 7_200);
        assert_eq!(dial["spans"].as_array().unwrap().len(), 3);

        let bad = Qth { name: "x".into(), lat: 91.0, lon: 0.0 };
        assert!(atlas.call(Call::SetQth(bad)).await.is_err());
    }

    #[tokio::test]
    async fn sim_rig_is_driven_through_calls() {
        let atlas = Atlas::start(AppConfig::default(), None, None, no_updates()).unwrap();
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

    /// Poll the scan status until `f` holds (10 s at most).
    async fn scan_until(atlas: &Atlas, f: impl Fn(&ScanStatus) -> bool) -> ScanStatus {
        for _ in 0..1000 {
            let s = atlas.scan.status();
            if f(&s) {
                return s;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("scan status stuck at {:?}", atlas.scan.status());
    }

    async fn tuned(atlas: &Atlas, hz: u32) {
        let mut rx = atlas.rig.subscribe();
        while atlas.rig.state().freq_hz != Some(hz) {
            rx.changed().await.unwrap();
        }
    }

    #[tokio::test]
    async fn scans_run_save_stop_and_delete() {
        let dir = std::env::temp_dir().join(format!("swatlas-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let scans = dir.join("scans");
        let atlas = Atlas::start(AppConfig::default(), None, Some(dir.clone()), no_updates()).unwrap();
        let mut rx = atlas.rig.subscribe();
        while atlas.rig.state().power != Power::On {
            rx.changed().await.unwrap();
        }
        atlas.call(Call::Rig(RigCommand::SetFreq { hz: 5_025_000 })).await.unwrap();
        tuned(&atlas, 5_025_000).await;
        let req = |band: &str, step_hz| ScanRequest { band_id: band.into(), step_hz, dwell_ms: Some(100) };

        // checked against the dial
        let e = atlas.call(Call::StartScan(req("nope", None))).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Invalid);
        let e = atlas.call(Call::StartScan(req("ham-10m", Some(500)))).await.unwrap_err();
        assert!(e.message.contains("too many"), "{}", e.message);

        // 2200 m: three 1 kHz steps, then back to 5025 kHz
        let s = atlas.call(Call::StartScan(req("ham-2200m", None))).await.unwrap();
        assert_eq!((s["state"].as_str(), s["total"].as_u64()), (Some("running"), Some(3)));
        let e = atlas.call(Call::StartScan(req("ham-630m", None))).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Conflict);
        // each step measured is a ping, newest last
        let ScanStatus::Running { pings, .. } =
            scan_until(&atlas, |s| matches!(s, ScanStatus::Running { done, .. } if *done >= 2)).await
        else {
            panic!("{:?}", atlas.scan.status())
        };
        let ix: Vec<u32> = pings.iter().map(|p| p.index).collect();
        assert!(ix.len() >= 2 && ix.windows(2).all(|w| w[0] < w[1]), "{ix:?}");
        let ScanStatus::Done { id, sites } = scan_until(&atlas, |s| !matches!(s, ScanStatus::Running { .. })).await
        else {
            panic!("{:?}", atlas.scan.status())
        };
        assert!(sites.is_empty(), "nothing broadcasts on 2200 m");
        tuned(&atlas, 5_025_000).await;
        let list = atlas.call(Call::Scans).await.unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        let first = &list[0];
        assert_eq!((first["id"].as_str(), first["band_id"].as_str()), (Some(id.as_str()), Some("ham-2200m")));
        assert_eq!((first["first_hz"].as_u64(), first["measured"].as_u64()), (Some(136_000), Some(3)));
        assert_eq!((first["outcome"]["kind"].as_str(), first["rig"].as_str()), (Some("complete"), Some("sim")));
        let scan = atlas.call(Call::Scan { id: id.clone() }).await.unwrap();
        assert!(scan["samples"].as_array().unwrap().iter().all(|v| v.is_i64()), "{scan}");
        assert!(scans.join(format!("{id}.json")).is_file());
        assert!(atlas.call(Call::Logbook).await.unwrap().is_array());

        // stopped by hand after a step: kept, and back where it was
        atlas.call(Call::StartScan(req("ham-630m", None))).await.unwrap();
        scan_until(&atlas, |s| matches!(s, ScanStatus::Running { done, .. } if *done >= 1)).await;
        atlas.call(Call::StopScan).await.unwrap();
        scan_until(&atlas, |s| matches!(s, ScanStatus::Done { .. })).await;
        tuned(&atlas, 5_025_000).await;
        let stopped = atlas.scan.summaries()[0].clone();
        assert_eq!(
            (stopped.band_id.as_str(), stopped.outcome.clone()),
            ("ham-630m", atlas_core::scan::ScanOutcome::Stopped)
        );
        assert!(stopped.measured < 8);

        // tuning takes the rig over and it stays there
        atlas.call(Call::StartScan(req("ham-630m", None))).await.unwrap();
        scan_until(&atlas, |s| matches!(s, ScanStatus::Running { done, .. } if *done >= 1)).await;
        atlas.call(Call::Rig(RigCommand::SetFreq { hz: 9_500_000 })).await.unwrap();
        tuned(&atlas, 9_500_000).await;
        scan_until(&atlas, |s| matches!(s, ScanStatus::Done { .. })).await;
        assert_eq!(atlas.rig.state().freq_hz, Some(9_500_000));

        // a new instance reads them back; deleting removes the file
        let again = Atlas::start(AppConfig::default(), None, Some(dir.clone()), no_updates()).unwrap();
        assert_eq!(again.call(Call::Scans).await.unwrap().as_array().unwrap().len(), 3);
        again.call(Call::DeleteScan { id: id.clone() }).await.unwrap();
        assert!(!scans.join(format!("{id}.json")).exists());
        assert_eq!(again.call(Call::DeleteScan { id }).await.unwrap_err().kind, ErrorKind::Invalid);
        assert_eq!(again.call(Call::Scans).await.unwrap().as_array().unwrap().len(), 2);
        assert_eq!(again.call(Call::DeleteScans).await.unwrap(), 2);
        assert!(again.call(Call::Scans).await.unwrap().as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn favorites_and_lenses_through_calls() {
        use atlas_core::favorites::FavoriteRequest;
        use atlas_core::rig::Mode;
        use atlas_core::stations::{Lens, Sort};

        let dir = std::env::temp_dir().join(format!("swatlas-favs-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let atlas = Atlas::start(AppConfig::default(), None, Some(dir.clone()), no_updates()).unwrap();
        let page = atlas.call(Call::Search(StationQuery { limit: 1, ..Default::default() })).await.unwrap();
        let st = &page["items"][0];
        let (id, freq_hz) = (st["id"].as_u64().unwrap() as u32, st["freq_hz"].as_u64().unwrap() as u32);
        let req = FavoriteRequest { freq_hz, mode: Mode::AM, label: None, station_id: Some(id) };
        let fav = atlas.call(Call::AddFavorite(req)).await.unwrap();
        assert_eq!(fav["label"], st["name"]);
        let free =
            FavoriteRequest { freq_hz: 14_230_000, mode: Mode::USB, label: Some("SSTV".into()), station_id: None };
        let free = atlas.call(Call::AddFavorite(free)).await.unwrap();
        let fav_id = fav["id"].as_str().unwrap().to_string();
        atlas.call(Call::RenameFavorite { id: fav_id.clone(), label: "Mine".into() }).await.unwrap();

        // the favorites lens: the station's row, and the free frequency off the schedule
        let favs = StationQuery { lens: Lens::Favorites, ..Default::default() };
        let page = atlas.call(Call::Search(favs.clone())).await.unwrap();
        assert_eq!((page["total"].as_u64(), page["items"][0]["id"].as_u64()), (Some(1), Some(u64::from(id))));
        assert_eq!(page["off_schedule"][0]["label"], "SSTV");
        // nothing heard yet: the Heard list is empty, in any order
        let q = StationQuery { lens: Lens::Heard, sort: Sort::Strongest, ..Default::default() };
        let heard = atlas.call(Call::Search(q)).await.unwrap();
        assert_eq!((heard["total"].as_u64(), heard["heard"].as_array().map(Vec::len)), (Some(0), Some(0)));

        // kept across a restart
        let again = Atlas::start(AppConfig::default(), None, Some(dir.clone()), no_updates()).unwrap();
        let list = again.call(Call::Favorites).await.unwrap();
        let labels: Vec<&str> =
            list.as_array().unwrap().iter().map(|v| v["favorite"]["label"].as_str().unwrap()).collect();
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"Mine") && labels.contains(&"SSTV"));
        let mine = list.as_array().unwrap().iter().find(|v| v["favorite"]["label"] == "Mine").unwrap();
        assert_eq!(mine["station_id"].as_u64(), Some(u64::from(id)));
        again.call(Call::DeleteFavorite { id: free["id"].as_str().unwrap().into() }).await.unwrap();
        let e = again.call(Call::DeleteFavorite { id: "nope".into() }).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Invalid);
        let page = again.call(Call::Search(favs)).await.unwrap();
        assert!(page["off_schedule"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn apply_audio_validates_switches_and_saves() {
        let dir = std::env::temp_dir().join(format!("swatlas-audio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("swatlas.toml");
        let atlas = Atlas::start(AppConfig::default(), Some(path.clone()), None, no_updates()).unwrap();

        let tuning = default_tuning(Os::CURRENT);
        let bad = AudioChoice { enabled: true, device: "-f lavfi".into(), rate: 16_000, tuning };
        let e = atlas.call(Call::ApplyAudio(bad)).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Invalid);
        assert!(!path.exists(), "a refused choice must not be saved");

        let tuning = atlas_core::audio::AudioTuning { cushion_ms: 250, ..tuning };
        let codec = AudioChoice { enabled: true, device: default_device(Os::CURRENT).into(), rate: 8_000, tuning };
        atlas.call(Call::ApplyAudio(codec.clone())).await.unwrap();
        let info = atlas.call(Call::Info).await.unwrap();
        assert_eq!((info["audio"].as_bool(), info["audio_rate"].as_u64()), (Some(true), Some(8_000)));
        assert_eq!((info["audio_cushion_ms"].as_u64(), info["audio_max_ahead_ms"].as_u64()), (Some(250), Some(500)));
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
        let atlas = Atlas::start(AppConfig::default(), Some(path.clone()), None, no_updates()).unwrap();
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

    #[tokio::test]
    async fn update_calls() {
        let dir = std::env::temp_dir().join(format!("swatlas-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("swatlas.toml");
        let fake = FakeUpdater::new(Packaging::Desktop { appimage: true }, Some("9.0.0"));
        let atlas = Atlas::start(AppConfig::default(), Some(path.clone()), None, fake.clone()).unwrap();

        let s = atlas.call(Call::UpdateStatus).await.unwrap();
        assert_eq!((s["state"]["state"].as_str(), s["install"]["kind"].as_str()), (Some("idle"), Some("auto")));
        let e = atlas.call(Call::InstallUpdate).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Conflict, "nothing offered yet");
        let s = atlas.call(Call::CheckUpdate).await.unwrap();
        assert_eq!(s["state"]["release"]["version"], "9.0.0");

        atlas.call(Call::InstallUpdate).await.unwrap();
        // the fake installs but can't restart the app: the failure says so
        while !matches!(atlas.update.status().state, UpdateState::Failed { .. }) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(fake.installed());

        atlas.call(Call::SetUpdatePrefs(UpdatePrefs { check: false, channel: Channel::Beta })).await.unwrap();
        let saved: AppConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!((saved.update.check, saved.update.channel), (false, Channel::Beta));
        let s = atlas.call(Call::UpdateStatus).await.unwrap();
        assert_eq!((s["check"].as_bool(), s["channel"].as_str()), (Some(false), Some("beta")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
