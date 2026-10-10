//! Update checks and installs, shared by both clients through `Atlas::call`.
//!
//! `UpdateService` keeps the state the UI shows and checks the channel's manifest at start
//! and every few hours. An `Updater` does the IO: `ManifestUpdater` (here) only reads the
//! manifest, for `swatlas-server` and the Linux `.deb`; the desktop shell brings one that
//! also downloads and installs (Tauri's updater, which verifies the signature).

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use atlas_core::platform::Os;
use atlas_core::update::{
    install_mode, is_newer, parse_manifest, Channel, InstallMode, Packaging, Release, UpdateEvent, UpdateState,
    UpdateStatus, CHECK_EVERY_SECS, FIRST_CHECK_SECS,
};
use tokio::sync::Notify;

use crate::config::UpdateConfig;

/// This build's version (the workspace version).
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Download progress: bytes so far, and the total if known.
pub type Progress = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

#[async_trait]
pub trait Updater: Send + Sync {
    fn packaging(&self) -> Packaging;
    /// The release in the manifest at `url`, if it is newer than this build.
    async fn check(&self, url: &str) -> Result<Option<Release>, String>;
    /// Download and verify the release found by the last `check`.
    async fn download(&self, progress: Progress) -> Result<(), String>;
    /// Install what `download` fetched and restart the app. Returns only on failure.
    async fn install(&self) -> Result<(), String>;
}

/// Reads the manifest only: the update is installed by hand.
pub struct ManifestUpdater {
    packaging: Packaging,
}

impl ManifestUpdater {
    pub fn new(packaging: Packaging) -> Self {
        Self { packaging }
    }
}

#[cfg(feature = "update-check")]
async fn fetch(url: &str) -> Result<String, String> {
    use std::sync::OnceLock;
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    let http = HTTP.get_or_init(|| {
        // reqwest is built without a crypto provider of its own: use ring, like Tauri's updater
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .user_agent(concat!("swatlas/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP client")
    });
    async { http.get(url).send().await?.error_for_status()?.text().await }
        .await
        .map_err(|e| format!("could not read {url}: {e}"))
}

#[cfg(not(feature = "update-check"))]
async fn fetch(_: &str) -> Result<String, String> {
    Err("built without update checks".into())
}

#[async_trait]
impl Updater for ManifestUpdater {
    fn packaging(&self) -> Packaging {
        self.packaging
    }

    async fn check(&self, url: &str) -> Result<Option<Release>, String> {
        let release = parse_manifest(&fetch(url).await?)?;
        Ok(is_newer(CURRENT, &release.version).then_some(release))
    }

    async fn download(&self, _: Progress) -> Result<(), String> {
        Err("install this update by hand".into())
    }

    async fn install(&self) -> Result<(), String> {
        Err("install this update by hand".into())
    }
}

/// Why an install can't start.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallRefused {
    /// This copy is updated by hand.
    Manual,
    /// Nothing to install, or an install is already running.
    NotNow(&'static str),
}

pub struct UpdateService {
    updater: Arc<dyn Updater>,
    install: InstallMode,
    url: Option<String>,
    locked: Vec<String>,
    inner: Mutex<Inner>,
    /// One check at a time.
    checking: tokio::sync::Mutex<()>,
    /// Check now (preferences changed).
    wake: Notify,
}

struct Inner {
    state: UpdateState,
    check: bool,
    channel: Channel,
    checked_at: Option<u64>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl UpdateService {
    pub fn new(updater: Arc<dyn Updater>, cfg: &UpdateConfig, locked: Vec<String>) -> Arc<Self> {
        Arc::new(Self {
            install: install_mode(Os::CURRENT, updater.packaging()),
            updater,
            url: cfg.url.clone(),
            locked,
            inner: Mutex::new(Inner {
                state: if cfg.check { UpdateState::Idle } else { UpdateState::Disabled },
                check: cfg.check,
                channel: cfg.channel,
                checked_at: None,
            }),
            checking: tokio::sync::Mutex::new(()),
            wake: Notify::new(),
        })
    }

    /// Check a little after start, then every few hours, while automatic checks are on.
    /// Needs a tokio runtime.
    pub fn spawn_checks(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut wait = Duration::from_secs(FIRST_CHECK_SECS);
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(wait) => wait = Duration::from_secs(CHECK_EVERY_SECS),
                    _ = me.wake.notified() => {}
                }
                if me.inner.lock().expect("update lock").check {
                    me.check_now().await;
                }
            }
        });
    }

    pub fn status(&self) -> UpdateStatus {
        let inner = self.inner.lock().expect("update lock");
        UpdateStatus {
            current: CURRENT.into(),
            check: inner.check,
            channel: inner.channel,
            install: self.install,
            state: inner.state.clone(),
            checked_at: inner.checked_at,
            locked: self.locked.clone(),
        }
    }

    fn apply(&self, ev: UpdateEvent) {
        let mut inner = self.inner.lock().expect("update lock");
        inner.state = std::mem::replace(&mut inner.state, UpdateState::Idle).next(ev);
    }

    fn manifest_url(&self) -> String {
        let channel = self.inner.lock().expect("update lock").channel;
        self.url.clone().unwrap_or_else(|| channel.manifest_url().into())
    }

    /// Look for a newer release now (also when automatic checks are off).
    pub async fn check_now(&self) -> UpdateStatus {
        let _one = self.checking.lock().await;
        self.apply(UpdateEvent::CheckStarted);
        let url = self.manifest_url();
        match self.updater.check(&url).await {
            Ok(found) => {
                self.apply(UpdateEvent::Found(found));
                self.inner.lock().expect("update lock").checked_at = Some(now_secs());
            }
            Err(e) => {
                tracing::info!("update check: {e}");
                self.apply(UpdateEvent::CheckFailed(e));
            }
        }
        self.status()
    }

    /// Change the preferences; a newly enabled check or a new channel checks right away.
    pub fn set_prefs(&self, check: bool, channel: Channel) {
        let changed = {
            let mut inner = self.inner.lock().expect("update lock");
            let changed = (inner.check, inner.channel) != (check, channel);
            if inner.channel != channel && !inner.state.busy() {
                // what the old channel offered may not be on the new one
                inner.state = UpdateState::Idle;
            }
            inner.check = check;
            inner.channel = channel;
            changed
        };
        self.apply(UpdateEvent::Enabled(check));
        if changed && check {
            self.wake.notify_one();
        }
    }

    /// Claim the offered release for an install (refused when there is nothing to install,
    /// one is running, or this copy is updated by hand). Follow with `run_install`.
    pub fn begin_install(&self) -> Result<(), InstallRefused> {
        if self.install != InstallMode::Auto {
            return Err(InstallRefused::Manual);
        }
        let mut inner = self.inner.lock().expect("update lock");
        if inner.state.busy() {
            return Err(InstallRefused::NotNow("an update is already being installed"));
        }
        if inner.state.installable().is_none() {
            return Err(InstallRefused::NotNow("there is no update to install"));
        }
        inner.state = std::mem::replace(&mut inner.state, UpdateState::Idle).next(UpdateEvent::DownloadStarted);
        Ok(())
    }

    /// Download, then `stop` the helpers (the audio capture, and rigctld: its file is about to
    /// be replaced) and install. The app restarts on success; on failure `resume` brings the
    /// helpers back and the state says why.
    pub async fn run_install(self: Arc<Self>, stop: impl Future<Output = ()>, resume: impl Future<Output = ()>) {
        let me = self.clone();
        let progress: Progress = Box::new(move |done, total| me.apply(UpdateEvent::Progress { done, total }));
        if let Err(e) = self.updater.download(progress).await {
            tracing::warn!("update download: {e}");
            self.apply(UpdateEvent::InstallFailed(e));
            return;
        }
        self.apply(UpdateEvent::InstallStarted);
        stop.await;
        let e = match self.updater.install().await {
            Ok(()) => "the app did not restart".to_string(),
            Err(e) => e,
        };
        tracing::warn!("update install: {e}");
        resume.await;
        self.apply(UpdateEvent::InstallFailed(e));
    }
}

/// An `Updater` for tests: offers `offer`, and installs or fails as told.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub struct FakeUpdater {
        pub packaging: Packaging,
        pub offer: Mutex<Result<Option<Release>, String>>,
        pub install_error: Option<String>,
        pub installed: AtomicBool,
        pub urls: Mutex<Vec<String>>,
    }

    impl FakeUpdater {
        pub fn new(packaging: Packaging, offer: Option<&str>) -> Arc<Self> {
            let release = offer.map(|v| Release {
                version: v.into(),
                notes: "notes".into(),
                pub_date: None,
                url: atlas_core::update::RELEASES_PAGE.into(),
            });
            Arc::new(Self {
                packaging,
                offer: Mutex::new(Ok(release)),
                install_error: None,
                installed: AtomicBool::new(false),
                urls: Mutex::new(vec![]),
            })
        }

        pub fn installed(&self) -> bool {
            self.installed.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl Updater for FakeUpdater {
        fn packaging(&self) -> Packaging {
            self.packaging
        }
        async fn check(&self, url: &str) -> Result<Option<Release>, String> {
            self.urls.lock().unwrap().push(url.into());
            self.offer.lock().unwrap().clone()
        }
        async fn download(&self, progress: Progress) -> Result<(), String> {
            progress(50, Some(100));
            progress(100, Some(100));
            Ok(())
        }
        async fn install(&self) -> Result<(), String> {
            match &self.install_error {
                Some(e) => Err(e.clone()),
                None => {
                    self.installed.store(true, Ordering::SeqCst);
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeUpdater;
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn desktop() -> Packaging {
        // auto-install on every OS, also Linux
        Packaging::Desktop { appimage: true }
    }

    #[tokio::test]
    async fn check_finds_and_refuses_installs_it_cannot_do() {
        let u = FakeUpdater::new(Packaging::Server, Some("9.0.0"));
        let svc = UpdateService::new(u.clone(), &UpdateConfig::default(), vec![]);
        assert_eq!(svc.status().state, UpdateState::Idle);
        assert_eq!(svc.begin_install(), Err(InstallRefused::Manual));
        let s = svc.check_now().await;
        assert!(matches!(&s.state, UpdateState::Available { release } if release.version == "9.0.0"));
        assert!(s.checked_at.is_some());
        assert_eq!(u.urls.lock().unwrap()[0], Channel::Stable.manifest_url());
        assert_eq!(svc.begin_install(), Err(InstallRefused::Manual), "the server is updated by hand");
    }

    #[tokio::test]
    async fn check_failure_and_url_override() {
        let u = FakeUpdater::new(desktop(), None);
        *u.offer.lock().unwrap() = Err("offline".into());
        let cfg = UpdateConfig { url: Some("https://example.org/rc.json".into()), ..UpdateConfig::default() };
        let svc = UpdateService::new(u.clone(), &cfg, vec![]);
        let s = svc.check_now().await;
        assert_eq!(s.state, UpdateState::Failed { message: "offline".into(), release: None });
        assert_eq!(s.checked_at, None);
        assert_eq!(u.urls.lock().unwrap()[0], "https://example.org/rc.json");
        assert_eq!(svc.begin_install(), Err(InstallRefused::NotNow("there is no update to install")));
    }

    #[tokio::test]
    async fn prefs_switch_channel_and_checks() {
        let u = FakeUpdater::new(desktop(), Some("9.0.0"));
        let svc = UpdateService::new(u.clone(), &UpdateConfig { check: false, ..UpdateConfig::default() }, vec![]);
        assert_eq!(svc.status().state, UpdateState::Disabled);
        svc.check_now().await;
        svc.set_prefs(true, Channel::Beta);
        let s = svc.status();
        assert_eq!((s.check, s.channel, s.state), (true, Channel::Beta, UpdateState::Idle), "the offer was for stable");
        svc.set_prefs(false, Channel::Beta);
        assert_eq!(svc.status().state, UpdateState::Disabled);
    }

    #[tokio::test]
    async fn install_stops_helpers_then_installs() {
        let u = FakeUpdater::new(desktop(), Some("9.0.0"));
        let svc = UpdateService::new(u.clone(), &UpdateConfig::default(), vec![]);
        svc.check_now().await;
        svc.begin_install().unwrap();
        assert!(matches!(svc.status().state, UpdateState::Downloading { done: 0, .. }));
        assert!(svc.begin_install().is_err(), "one install at a time");

        let (stopped, resumed) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let (s2, r2) = (stopped.clone(), resumed.clone());
        svc.clone()
            .run_install(
                async move { s2.store(true, Ordering::SeqCst) },
                async move { r2.store(true, Ordering::SeqCst) },
            )
            .await;
        assert!(stopped.load(Ordering::SeqCst) && u.installed());
        // the fake "installs" but can't restart: that is reported, and the helpers come back
        assert!(resumed.load(Ordering::SeqCst));
        assert!(matches!(svc.status().state, UpdateState::Failed { release: Some(_), .. }));
    }

    #[tokio::test]
    async fn failed_install_can_be_retried() {
        let mut fake = Arc::into_inner(FakeUpdater::new(desktop(), Some("9.0.0"))).unwrap();
        fake.install_error = Some("signature mismatch".into());
        let u = Arc::new(fake);
        let svc = UpdateService::new(u.clone(), &UpdateConfig::default(), vec![]);
        svc.check_now().await;
        svc.begin_install().unwrap();
        svc.clone().run_install(async {}, async {}).await;
        assert!(matches!(
            svc.status().state,
            UpdateState::Failed { message, release: Some(r) } if message == "signature mismatch" && r.version == "9.0.0"
        ));
        assert!(svc.begin_install().is_ok(), "retry");
    }
}
