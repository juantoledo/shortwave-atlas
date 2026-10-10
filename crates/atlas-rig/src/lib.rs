//! Rig access for Shortwave Atlas: backends (Hamlib `rigctld`, simulator), the poller that
//! publishes `RigState` changes, and the `rigctld` supervisor.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use atlas_core::api::{Qth, RigDiagnostics};
use atlas_core::rig::{default_coverage, CommandError, Link, Mode, Power, RigCommand, RigState, RxRange};
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
/// How long a command waits for a scan it stopped to let go of the rig (one step at most).
const SCAN_RELEASE: Duration = Duration::from_secs(5);

#[async_trait]
pub trait RigBackend: Send + Sync {
    /// Current state; `last` lets a backend skip queries (e.g. powerstat while on).
    async fn get_state(&self, last: &RigState) -> RigState;
    async fn set_freq(&self, hz: u32) -> Result<(), CommandError>;
    async fn set_mode(&self, mode: Mode) -> Result<(), CommandError>;
    async fn set_power(&self, on: bool) -> Result<(), CommandError>;
    /// The S-meter alone, in dB relative to S9 (a scan reads it at every step).
    async fn strength(&self) -> Option<i32> {
        self.get_state(&RigState::without_rig(Link::Ok, Power::On)).await.strength_db
    }
    /// What the rig receives, if it can say.
    async fn coverage(&self) -> Option<Vec<RxRange>> {
        None
    }
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
    /// A scan holds the rig: the poller sleeps.
    paused: Arc<AtomicBool>,
    tasks: Vec<JoinHandle<()>>,
    supervisor: Option<rigctld::Supervisor>,
    /// The backend's coverage, once it has answered.
    coverage: Arc<Mutex<Option<Vec<RxRange>>>>,
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

/// Why a scan stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanStop {
    /// Asked to (`Rig::stop_scan`): the rig goes back where it was.
    User,
    /// Another command took the rig over: it stays where that put it.
    Takeover,
    /// The backend is switching or going away.
    Lost,
}

/// A running scan's controls, shared by the `Rig` and the `ScanLink`.
struct ScanCtl {
    stop: Mutex<Option<ScanStop>>,
    /// Becomes `true` when the `ScanLink` is gone.
    released: watch::Sender<bool>,
}

type ScanSlot = Arc<Mutex<Option<Arc<ScanCtl>>>>;

/// The rig, lent to a scan (`Rig::begin_scan`): the poller sleeps and only the scan talks to
/// the rig, publishing each frequency and reading as the state. Dropping it hands the rig back.
pub struct ScanLink {
    backend: Arc<dyn RigBackend>,
    state: Arc<watch::Sender<RigState>>,
    paused: Arc<AtomicBool>,
    wake: Arc<Notify>,
    ctl: Arc<ScanCtl>,
    slot: ScanSlot,
}

impl ScanLink {
    /// Set once something stops the scan; check it before each step.
    pub fn stopped(&self) -> Option<ScanStop> {
        *self.ctl.stop.lock().expect("scan lock")
    }

    /// Tune to `hz`, wait `dwell` for the AGC, then read the meter `reads` times `gap` apart:
    /// the strongest reading (`None` if the rig gave none).
    pub async fn measure(&self, hz: u32, dwell: Duration, reads: u32, gap: Duration) -> Result<Option<i32>, CommandError> {
        self.backend.set_freq(hz).await?;
        tokio::time::sleep(dwell).await;
        let mut best: Option<i32> = None;
        for k in 0..reads {
            if k > 0 {
                tokio::time::sleep(gap).await;
            }
            if let Some(db) = self.backend.strength().await {
                best = Some(best.map_or(db, |b| b.max(db)));
            }
        }
        self.state.send_if_modified(|s| {
            let changed = (s.freq_hz, s.strength_db) != (Some(hz), best);
            (s.freq_hz, s.strength_db) = (Some(hz), best);
            changed
        });
        Ok(best)
    }

    /// Tune without reading (back where the scan started).
    pub async fn tune(&self, hz: u32) -> Result<(), CommandError> {
        self.backend.set_freq(hz).await
    }
}

impl Drop for ScanLink {
    fn drop(&mut self) {
        {
            let mut slot = self.slot.lock().expect("scan lock");
            if slot.as_ref().is_some_and(|c| Arc::ptr_eq(c, &self.ctl)) {
                *slot = None;
            }
        }
        self.paused.store(false, Ordering::SeqCst);
        self.wake.notify_one();
        self.ctl.released.send_replace(true);
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
    /// The running scan, if any.
    scan: ScanSlot,
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
            scan: Default::default(),
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
        let paused = Arc::new(AtomicBool::new(false));
        let tasks = vec![
            tokio::spawn(poll(backend.clone(), self.state.clone(), wake.clone(), paused.clone())),
            tokio::spawn(tune(backend.clone(), freq_rx, wake.clone())),
        ];
        Running { kind, backend, freq, wake, paused, tasks, supervisor, coverage: Default::default() }
    }

    /// Stop the current backend (waiting for it), then start the one in `cfg`.
    pub async fn reconfigure(&self, cfg: &RigConfig) {
        self.stop_scan(ScanStop::Lost).await;
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
        self.stop_scan(ScanStop::Lost).await;
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

    /// What the rig receives: asked once per backend (until it answers), else the default.
    pub async fn coverage(&self) -> Vec<RxRange> {
        let (backend, cache) = {
            let guard = self.running.lock().expect("rig lock");
            let Some(r) = guard.as_ref() else { return default_coverage() };
            (r.backend.clone(), r.coverage.clone())
        };
        if let Some(c) = cache.lock().expect("coverage lock").clone() {
            return c;
        }
        let answer = backend.coverage().await;
        // a reachable rigctld that cannot say keeps the fallback, rather than being asked on every tune
        if answer.is_some() || self.state.borrow().link != Link::Down {
            *cache.lock().expect("coverage lock") = Some(answer.clone().unwrap_or_else(default_coverage));
        }
        answer.unwrap_or_else(default_coverage)
    }

    /// Lend the rig to a scan until the link is dropped (refused while it is off, or while
    /// another scan has it).
    pub fn begin_scan(&self) -> Result<ScanLink, CommandError> {
        let guard = self.running.lock().expect("rig lock");
        let Some(r) = guard.as_ref() else {
            return Err(CommandError::Conflict("Not connected".into()));
        };
        if self.state.borrow().power != Power::On {
            return Err(CommandError::Conflict("The radio is off".into()));
        }
        let mut slot = self.scan.lock().expect("scan lock");
        if slot.is_some() {
            return Err(CommandError::Conflict("A scan is already running".into()));
        }
        let ctl = Arc::new(ScanCtl { stop: Mutex::new(None), released: watch::channel(false).0 });
        *slot = Some(ctl.clone());
        r.paused.store(true, Ordering::SeqCst);
        Ok(ScanLink {
            backend: r.backend.clone(),
            state: self.state.clone(),
            paused: r.paused.clone(),
            wake: r.wake.clone(),
            ctl,
            slot: self.scan.clone(),
        })
    }

    pub fn scanning(&self) -> bool {
        self.scan.lock().expect("scan lock").is_some()
    }

    /// Stop the running scan, if any, and wait (a few seconds at most) until it lets go of the
    /// rig. The first reason given is the one that counts.
    pub async fn stop_scan(&self, why: ScanStop) {
        let Some(ctl) = self.scan.lock().expect("scan lock").clone() else { return };
        ctl.stop.lock().expect("scan lock").get_or_insert(why);
        let mut released = ctl.released.subscribe();
        let _ = tokio::time::timeout(SCAN_RELEASE, released.wait_for(|r| *r)).await;
    }

    /// Run a command. Any command takes the rig over from a running scan.
    pub async fn execute(&self, cmd: RigCommand) -> Result<(), CommandError> {
        self.stop_scan(ScanStop::Takeover).await;
        if let RigCommand::SetFreq { .. } = cmd {
            cmd.validate(&self.coverage().await)?;
        }
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

async fn poll(backend: Arc<dyn RigBackend>, tx: Arc<watch::Sender<RigState>>, wake: Arc<Notify>, paused: Arc<AtomicBool>) {
    let mut last = RigState::down();
    loop {
        // while a scan has the rig, it publishes the state itself
        if !paused.load(Ordering::SeqCst) {
            let s = backend.get_state(&last).await;
            tx.send_if_modified(|cur| {
                let changed = *cur != s;
                if changed {
                    *cur = s.clone();
                }
                changed
            });
            last = s;
        }
        let delay = if last.power == Power::On { POLL_ON } else { POLL_OFF };
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
    async fn tuning_follows_the_rig_coverage() {
        let fake = FakeRigctld::start().await;
        let rig = Rig::start(Arc::new(hamlib::Hamlib::new("127.0.0.1", fake.port)));
        let mut rx = rig.subscribe();
        changed(&mut rx).await;
        // the fake receives up to 470 MHz, past the 56 MHz fallback
        rig.execute(RigCommand::SetFreq { hz: 145_500_000 }).await.unwrap();
        until(&mut rx, |s| s.freq_hz == Some(145_500_000)).await;
        let e = rig.execute(RigCommand::SetFreq { hz: 500_000_000 }).await.unwrap_err();
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

    const MS: Duration = Duration::from_millis(1);

    #[tokio::test]
    async fn a_scan_has_the_rig_to_itself() {
        let fake = FakeRigctld::start().await;
        fake.signal(5_905_000, -12).await;
        let rig = Rig::start(Arc::new(hamlib::Hamlib::new("127.0.0.1", fake.port)));
        let mut rx = rig.subscribe();
        until(&mut rx, |s| s.power == Power::On).await;

        let link = rig.begin_scan().unwrap();
        assert!(rig.scanning());
        assert_eq!(rig.begin_scan().err(), Some(CommandError::Conflict("A scan is already running".into())));
        // a poll already under way finishes; no other starts
        tokio::time::sleep(300 * MS).await;
        let from = fake.all().await.len();
        let mut got = vec![];
        for hz in [5_900_000, 5_905_000, 5_910_000] {
            got.push(link.measure(hz, 30 * MS, 3, MS).await.unwrap());
            // each reading is published as the state
            assert_eq!((rig.state().freq_hz, rig.state().strength_db), (Some(hz), *got.last().unwrap()));
        }
        assert_eq!(got, [Some(-54), Some(-12), Some(-54)]);
        // the poller slept: only tuning and meter reads reached the rig
        let during = fake.all().await[from..].to_vec();
        assert_eq!(during.len(), 12, "{during:?}");
        assert!(during.iter().all(|c| c.starts_with("F ") || c == "l STRENGTH"), "{during:?}");

        drop(link);
        assert!(!rig.scanning());
        rig.begin_scan().unwrap();
    }

    #[tokio::test]
    async fn commands_take_the_rig_from_a_scan() {
        let fake = FakeRigctld::start().await;
        let rig = Arc::new(Rig::start(Arc::new(hamlib::Hamlib::new("127.0.0.1", fake.port))));
        let mut rx = rig.subscribe();
        until(&mut rx, |s| s.power == Power::On).await;

        let link = rig.begin_scan().unwrap();
        // a scan step: stop it when told, then let go of the rig
        let job = tokio::spawn(async move {
            while link.stopped().is_none() {
                link.measure(7_100_000, 5 * MS, 1, MS).await.unwrap();
            }
            link.stopped()
        });
        rig.execute(RigCommand::SetFreq { hz: 9_500_000 }).await.unwrap();
        assert_eq!(job.await.unwrap(), Some(ScanStop::Takeover));
        assert!(!rig.scanning());
        // the command came after the scan's last step
        let s = until(&mut rx, |s| s.freq_hz == Some(9_500_000)).await;
        assert_eq!(s.power, Power::On);
        assert_eq!(fake.log().await.last().map(String::as_str), Some("F 9500000"));

        // stopping by hand says so; a switch says the rig was lost
        let link = rig.begin_scan().unwrap();
        let r = rig.clone();
        let stop = tokio::spawn(async move { r.stop_scan(ScanStop::User).await });
        while link.stopped().is_none() {
            tokio::time::sleep(MS).await;
        }
        assert_eq!(link.stopped(), Some(ScanStop::User));
        drop(link);
        stop.await.unwrap();
        let _link = rig.begin_scan().unwrap();
        rig.disconnect().await;
        assert_eq!(_link.stopped(), Some(ScanStop::Lost));
    }

    #[tokio::test]
    async fn no_scan_while_off() {
        let fake = FakeRigctld::start().await;
        let rig = Rig::start(Arc::new(hamlib::Hamlib::new("127.0.0.1", fake.port)));
        let mut rx = rig.subscribe();
        until(&mut rx, |s| s.power == Power::On).await;
        rig.execute(RigCommand::SetPower { on: false }).await.unwrap();
        until(&mut rx, |s| s.power == Power::Off).await;
        assert!(matches!(rig.begin_scan(), Err(CommandError::Conflict(_))));
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
