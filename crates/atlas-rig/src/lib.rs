//! Rig access for Shortwave Atlas: backends (Hamlib `rigctld`, simulator), the poller that
//! publishes `RigState` changes, and the `rigctld` supervisor.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use atlas_core::api::{Qth, RigDiagnostics};
use atlas_core::rig::{CommandError, Link, Mode, Power, RigCommand, RigState};
use atlas_core::setup::diagnose;
pub use atlas_core::setup::{BackendKind, RigConfig};
use atlas_core::stations::Catalog;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

pub mod discover;
pub mod hamlib;
pub mod process;
pub mod rigctld;
pub mod sim;

#[cfg(test)]
mod fake;

/// Poll interval while the rig is on.
const POLL_ON: Duration = Duration::from_millis(200);
/// Poll interval while it is off or unreachable (each poll can take seconds then).
const POLL_OFF: Duration = Duration::from_secs(2);

#[async_trait]
pub trait RigBackend: Send + Sync {
    /// Current state; `last` lets a backend skip queries (e.g. powerstat while on).
    async fn get_state(&self, last: &RigState) -> RigState;
    async fn set_freq(&self, hz: u32) -> Result<(), CommandError>;
    async fn set_mode(&self, mode: Mode) -> Result<(), CommandError>;
    async fn set_power(&self, on: bool) -> Result<(), CommandError>;
    /// Last error talking to the rig, for diagnostics.
    fn last_error(&self) -> Option<String> {
        None
    }
}

/// One connected backend with its tasks.
struct Running {
    kind: BackendKind,
    backend: Arc<dyn RigBackend>,
    freq: watch::Sender<Option<u32>>,
    wake: Arc<Notify>,
    tasks: Vec<JoinHandle<()>>,
    supervisor: Option<rigctld::Supervisor>,
}

impl Running {
    /// Stop the tasks and any rigctld, waiting until they are gone.
    async fn stop(self) {
        for t in self.tasks {
            t.abort();
            let _ = t.await;
        }
        if let Some(s) = self.supervisor {
            s.stop().await;
        }
    }
}

/// Handle to the rig: latest state, change notifications and commands.
///
/// The state channel lives as long as the `Rig`, so subscribers keep working when the
/// settings page switches backends. Dropping it stops everything, rigctld included.
pub struct Rig {
    state: Arc<watch::Sender<RigState>>,
    running: Mutex<Option<Running>>,
    /// Serialises reconfigure/disconnect.
    switching: tokio::sync::Mutex<()>,
    stations: Option<Arc<Catalog>>,
    qth: Option<Arc<RwLock<Qth>>>,
}

impl Rig {
    /// Build the backend from config and start it. Needs a tokio runtime.
    pub fn from_config(cfg: &RigConfig, stations: Arc<Catalog>, qth: Arc<RwLock<Qth>>) -> Self {
        let rig = Self::empty(Some(stations), Some(qth));
        let r = rig.launch(cfg);
        *rig.running.lock().expect("rig lock") = Some(r);
        rig
    }

    /// Start with a ready-made backend (tests).
    pub fn start(backend: Arc<dyn RigBackend>) -> Self {
        let rig = Self::empty(None, None);
        let r = rig.run(BackendKind::External, backend, None);
        *rig.running.lock().expect("rig lock") = Some(r);
        rig
    }

    fn empty(stations: Option<Arc<Catalog>>, qth: Option<Arc<RwLock<Qth>>>) -> Self {
        Self {
            state: Arc::new(watch::channel(RigState::down()).0),
            running: Mutex::new(None),
            switching: tokio::sync::Mutex::new(()),
            stations,
            qth,
        }
    }

    fn launch(&self, cfg: &RigConfig) -> Running {
        match cfg.backend {
            BackendKind::Sim => {
                let stations = self.stations.clone().expect("sim needs stations");
                let qth = self.qth.clone().expect("sim needs a QTH");
                self.run(BackendKind::Sim, Arc::new(sim::Sim::new(stations, qth)), None)
            }
            BackendKind::External => {
                self.run(BackendKind::External, Arc::new(hamlib::Hamlib::new(&cfg.host, cfg.port)), None)
            }
            BackendKind::Spawn => {
                let sup = rigctld::supervise(rigctld::Spec::from(cfg));
                self.run(BackendKind::Spawn, Arc::new(hamlib::Hamlib::new("127.0.0.1", cfg.port)), Some(sup))
            }
        }
    }

    fn run(&self, kind: BackendKind, backend: Arc<dyn RigBackend>, supervisor: Option<rigctld::Supervisor>) -> Running {
        let (freq, freq_rx) = watch::channel(None);
        let wake = Arc::new(Notify::new());
        let tasks = vec![
            tokio::spawn(poll(backend.clone(), self.state.clone(), wake.clone())),
            tokio::spawn(tune(backend.clone(), freq_rx, wake.clone())),
        ];
        Running { kind, backend, freq, wake, tasks, supervisor }
    }

    /// Stop the current backend (waiting for it), then start the one in `cfg`.
    pub async fn reconfigure(&self, cfg: &RigConfig) {
        let _g = self.switching.lock().await;
        let old = self.running.lock().expect("rig lock").take();
        if let Some(r) = old {
            r.stop().await;
        }
        self.state.send_replace(RigState::down());
        let r = self.launch(cfg);
        *self.running.lock().expect("rig lock") = Some(r);
    }

    /// Stop talking to the rig (and stop our rigctld) until the next `reconfigure`.
    pub async fn disconnect(&self) {
        let _g = self.switching.lock().await;
        let old = self.running.lock().expect("rig lock").take();
        if let Some(r) = old {
            r.stop().await;
        }
        self.state.send_replace(RigState::without_rig(Link::Off, Power::Unknown));
    }

    pub fn state(&self) -> RigState {
        self.state.borrow().clone()
    }

    /// Receiver that sees every state change, across backend switches.
    pub fn subscribe(&self) -> watch::Receiver<RigState> {
        self.state.subscribe()
    }

    /// What the settings page shows while connecting.
    pub fn diagnostics(&self) -> RigDiagnostics {
        let state = self.state();
        let guard = self.running.lock().expect("rig lock");
        let Some(r) = guard.as_ref() else {
            return RigDiagnostics { backend: BackendKind::Sim, rigctld: None, last_error: None, hints: vec![] };
        };
        let rigctld = r.supervisor.as_ref().map(rigctld::Supervisor::status);
        RigDiagnostics {
            backend: r.kind,
            hints: diagnose(atlas_core::platform::Os::CURRENT, r.kind, rigctld.as_ref(), &state),
            rigctld,
            last_error: r.backend.last_error(),
        }
    }

    pub async fn execute(&self, cmd: RigCommand) -> Result<(), CommandError> {
        cmd.validate()?;
        let (backend, wake) = {
            let guard = self.running.lock().expect("rig lock");
            let Some(r) = guard.as_ref() else {
                return Err(CommandError::Conflict("Not connected".into()));
            };
            if let RigCommand::SetFreq { hz } = cmd {
                if self.state.borrow().power != Power::On {
                    return Err(CommandError::Conflict("The radio is off".into()));
                }
                // Coalesced: the tuning task sends only the latest value, so a fast
                // dial drag cannot queue up hundreds of CAT writes.
                r.freq.send_replace(Some(hz));
                return Ok(());
            }
            (r.backend.clone(), r.wake.clone())
        };
        let res = match cmd {
            RigCommand::SetPower { on } => backend.set_power(on).await,
            _ if self.state.borrow().power != Power::On => Err(CommandError::Conflict("The radio is off".into())),
            RigCommand::SetMode { mode } => backend.set_mode(mode).await,
            RigCommand::SetFreq { .. } => unreachable!("handled above"),
        };
        wake.notify_one();
        res
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(r) = self.running.get_mut().ok().and_then(Option::take) {
            r.tasks.iter().for_each(JoinHandle::abort);
            // dropping the supervisor kills rigctld
        }
    }
}

async fn poll(backend: Arc<dyn RigBackend>, tx: Arc<watch::Sender<RigState>>, wake: Arc<Notify>) {
    let mut last = RigState::down();
    loop {
        let s = backend.get_state(&last).await;
        tx.send_if_modified(|cur| {
            let changed = *cur != s;
            if changed {
                *cur = s.clone();
            }
            changed
        });
        let delay = if s.power == Power::On { POLL_ON } else { POLL_OFF };
        last = s;
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = wake.notified() => {}
        }
    }
}

async fn tune(backend: Arc<dyn RigBackend>, mut rx: watch::Receiver<Option<u32>>, wake: Arc<Notify>) {
    while rx.changed().await.is_ok() {
        let Some(hz) = *rx.borrow_and_update() else { continue };
        if let Err(e) = backend.set_freq(hz).await {
            tracing::warn!("set_freq {hz}: {e}");
        }
        wake.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeRigctld;

    async fn changed(rx: &mut watch::Receiver<RigState>) -> RigState {
        tokio::time::timeout(Duration::from_secs(5), rx.changed()).await.unwrap().unwrap();
        rx.borrow_and_update().clone()
    }

    async fn until(rx: &mut watch::Receiver<RigState>, f: impl Fn(&RigState) -> bool) -> RigState {
        loop {
            let s = rx.borrow_and_update().clone();
            if f(&s) {
                return s;
            }
            changed(rx).await;
        }
    }

    #[tokio::test]
    async fn poller_publishes_and_commands_apply() {
        let fake = FakeRigctld::start().await;
        let rig = Rig::start(Arc::new(hamlib::Hamlib::new("127.0.0.1", fake.port)));
        let mut rx = rig.subscribe();
        let s = changed(&mut rx).await;
        assert_eq!((s.link, s.freq_hz), (Link::Ok, Some(14_074_000)));

        rig.execute(RigCommand::SetFreq { hz: 5_025_000 }).await.unwrap();
        let s = until(&mut rx, |s| s.freq_hz == Some(5_025_000)).await;
        assert_eq!(s.power, Power::On);
    }

    #[tokio::test]
    async fn fast_tuning_is_coalesced() {
        let fake = FakeRigctld::start().await;
        let rig = Rig::start(Arc::new(hamlib::Hamlib::new("127.0.0.1", fake.port)));
        let mut rx = rig.subscribe();
        changed(&mut rx).await;
        for k in 0..200 {
            rig.execute(RigCommand::SetFreq { hz: 7_000_000 + k * 100 }).await.unwrap();
        }
        until(&mut rx, |s| s.freq_hz == Some(7_019_900)).await;
        let writes = fake.log().await.len();
        assert!(writes < 20, "{writes} writes for 200 dial steps");
    }

    #[tokio::test]
    async fn commands_are_refused_while_off() {
        let fake = FakeRigctld::start().await;
        let rig = Rig::start(Arc::new(hamlib::Hamlib::new("127.0.0.1", fake.port)));
        let mut rx = rig.subscribe();
        changed(&mut rx).await;
        rig.execute(RigCommand::SetPower { on: false }).await.unwrap();
        until(&mut rx, |s| s.power == Power::Off).await;
        let e = rig.execute(RigCommand::SetMode { mode: Mode::AM }).await.unwrap_err();
        assert!(matches!(e, CommandError::Conflict(_)));
        let e = rig.execute(RigCommand::SetFreq { hz: 1 }).await.unwrap_err();
        assert!(matches!(e, CommandError::Invalid(_)));
    }

    #[tokio::test]
    async fn switching_backends_keeps_subscribers() {
        let rig = Rig::from_config(&RigConfig::default(), sim::tests::catalog(), Arc::new(RwLock::new(Qth::default())));
        let mut rx = rig.subscribe();
        until(&mut rx, |s| s.link == Link::Sim).await;

        let fake = FakeRigctld::start().await;
        let ext = RigConfig { backend: BackendKind::External, port: fake.port, ..RigConfig::default() };
        rig.reconfigure(&ext).await;
        let s = until(&mut rx, |s| s.link == Link::Ok).await;
        assert_eq!(s.freq_hz, Some(14_074_000));
        assert_eq!(rig.diagnostics().backend, BackendKind::External);

        rig.disconnect().await;
        until(&mut rx, |s| s.link == Link::Off).await;
        let e = rig.execute(RigCommand::SetMode { mode: Mode::AM }).await.unwrap_err();
        assert_eq!(e, CommandError::Conflict("Not connected".into()));

        rig.reconfigure(&RigConfig::default()).await;
        until(&mut rx, |s| s.link == Link::Sim).await;
    }

    #[tokio::test]
    async fn unreachable_external_rigctld_is_diagnosed() {
        let rig = Rig::start(Arc::new(hamlib::Hamlib::new("127.0.0.1", 1)));
        let mut rx = rig.subscribe();
        // first poll fails to connect: at once on Linux and macOS, but Windows retries a
        // refused connection for ~2 s before reporting it
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while rig.diagnostics().last_error.is_none() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(rx.borrow_and_update().link, Link::Down);
        let d = rig.diagnostics();
        assert!(d.last_error.is_some());
        assert_eq!(d.hints, [atlas_core::setup::Hint::RigctldUnreachable]);
    }
}
