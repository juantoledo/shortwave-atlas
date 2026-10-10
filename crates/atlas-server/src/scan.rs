//! Band scans: running one on the rig a `ScanLink` lends, and the saved ones, one JSON file each
//! in `scans/`, with the logbook built from them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use atlas_core::api::Qth;
use atlas_core::scan::{
    analyse, logbook, logged_sites, on_air_sites, scan_id, valid_id, Logged, Scan, ScanOutcome, ScanPing, ScanPlan,
    ScanStatus, ScanSummary, FORMAT_VERSION, PINGS_KEPT, READS, READ_GAP_MS,
};
use atlas_core::stations::Catalog;
use atlas_rig::{ScanLink, ScanStop};

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Where and how a scan ran, for its record.
pub struct Context {
    pub qth: Qth,
    /// Which rig (backend, model).
    pub rig: String,
    pub mode: Option<String>,
    /// Where the rig was before: it goes back there after the scan.
    pub back_hz: Option<u32>,
}

pub struct ScanService {
    /// `scans/`; `None` keeps scans in memory only (tests).
    dir: Option<PathBuf>,
    catalog: Arc<Catalog>,
    inner: Mutex<Inner>,
}

struct Inner {
    status: ScanStatus,
    /// Newest first.
    scans: Vec<Scan>,
    logbook: Vec<Logged>,
    logged: Arc<HashSet<u32>>,
}

impl Inner {
    fn relog(&mut self, catalog: &Catalog) {
        self.logbook = logbook(self.scans.iter().map(|s| &s.summary), catalog);
        self.logged = Arc::new(self.logbook.iter().map(|l| l.station_id).collect());
    }
}

/// Every scan file in `dir`, newest first; files that do not read are skipped.
fn load(dir: &Path) -> Vec<Scan> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut scans: Vec<Scan> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("skipping scan {}: {e}", p.display());
                None
            }
        })
        .collect();
    scans.sort_by(|a: &Scan, b| b.summary.started.cmp(&a.summary.started).then_with(|| b.summary.id.cmp(&a.summary.id)));
    scans
}

impl ScanService {
    /// Load the saved scans and their logbook (against `catalog`).
    pub fn new(dir: Option<PathBuf>, catalog: Arc<Catalog>) -> Arc<Self> {
        let scans = dir.as_deref().map(load).unwrap_or_default();
        let mut inner = Inner { status: ScanStatus::Idle, scans, logbook: vec![], logged: Default::default() };
        inner.relog(&catalog);
        Arc::new(Self { dir, catalog, inner: Mutex::new(inner) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("scan lock")
    }

    pub fn status(&self) -> ScanStatus {
        self.lock().status.clone()
    }

    pub fn summaries(&self) -> Vec<ScanSummary> {
        self.lock().scans.iter().map(|s| s.summary.clone()).collect()
    }

    pub fn scan(&self, id: &str) -> Option<Scan> {
        self.lock().scans.iter().find(|s| s.summary.id == id).cloned()
    }

    pub fn logbook(&self) -> Vec<Logged> {
        self.lock().logbook.clone()
    }

    /// Station ids in the logbook (the station list's "logged" filter).
    pub fn logged(&self) -> Arc<HashSet<u32>> {
        self.lock().logged.clone()
    }

    /// Forget a scan and remove its file. `false` if there is no such scan.
    pub fn delete(&self, id: &str) -> Result<bool, String> {
        if !valid_id(id) {
            return Ok(false);
        }
        let mut inner = self.lock();
        let Some(i) = inner.scans.iter().position(|s| s.summary.id == id) else { return Ok(false) };
        if let Some(dir) = &self.dir {
            match std::fs::remove_file(dir.join(format!("{id}.json"))) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                _ => {}
            }
        }
        inner.scans.remove(i);
        inner.relog(&self.catalog);
        Ok(true)
    }

    /// Write `scan` to `scans/<id>.json` (through a temporary file, so a crash leaves no half
    /// a file).
    fn save(&self, scan: &Scan) -> Result<(), String> {
        let Some(dir) = &self.dir else { return Ok(()) };
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.json", scan.summary.id));
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string(scan).expect("Scan serialises");
        std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())
    }

    /// Sweep `plan` on the rig `link` lends, in the background; the status says how it goes.
    pub fn start(self: &Arc<Self>, link: ScanLink, plan: ScanPlan, ctx: Context) -> ScanStatus {
        let started = now_secs();
        let base = scan_id(started, &plan.band.id);
        let mut inner = self.lock();
        // two scans of a band within a second: `...Z-49m`, `...Z-49m-2`
        let id = (1..)
            .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
            .find(|id| !inner.scans.iter().any(|s| &s.summary.id == id))
            .expect("some suffix is free");
        let status = ScanStatus::Running {
            id: id.clone(),
            band_id: plan.band.id.clone(),
            first_hz: plan.first_hz,
            step_hz: plan.step_hz,
            started,
            done: 0,
            total: plan.len() as u32,
            samples: vec![None; plan.len()],
            pings: vec![],
        };
        inner.status = status.clone();
        drop(inner);
        let me = self.clone();
        tokio::spawn(async move { me.run(link, plan, ctx, id, started).await });
        status
    }

    async fn run(&self, link: ScanLink, plan: ScanPlan, ctx: Context, id: String, started: i64) {
        let (dwell, gap) = (Duration::from_millis(plan.dwell_ms.into()), Duration::from_millis(READ_GAP_MS.into()));
        let mut times = vec![0i64; plan.len()];
        let mut failure = None;
        for i in 0..plan.len() {
            if link.stopped().is_some() {
                break;
            }
            if plan.covered[i] {
                match link.measure(plan.freq(i), dwell, READS, gap).await {
                    Ok(db) => {
                        times[i] = now_secs();
                        let db = db.map(|d| d.clamp(i16::MIN.into(), i16::MAX.into()) as i16);
                        let sites = on_air_sites(&self.catalog, plan.freq(i), plan.step_hz, times[i]);
                        if let ScanStatus::Running { samples, pings, .. } = &mut self.lock().status {
                            samples[i] = db;
                            if pings.len() == PINGS_KEPT {
                                pings.remove(0);
                            }
                            pings.push(ScanPing { index: i as u32, db, sites });
                        }
                    }
                    Err(e) => {
                        failure = Some(e.to_string());
                        break;
                    }
                }
            }
            if let ScanStatus::Running { done, .. } = &mut self.lock().status {
                *done = i as u32 + 1;
            }
        }
        let why = link.stopped();
        // back where the listener was, unless they tuned elsewhere or the rig went away
        if failure.is_none() && matches!(why, None | Some(ScanStop::User)) {
            if let Some(hz) = ctx.back_hz {
                if let Err(e) = link.tune(hz).await {
                    tracing::warn!("scan: retune to {hz}: {e}");
                }
            }
        }
        drop(link);

        let samples = match std::mem::replace(&mut self.lock().status, ScanStatus::Idle) {
            ScanStatus::Running { samples, .. } => samples,
            _ => vec![None; plan.len()],
        };
        let measured = samples.iter().filter(|s| s.is_some()).count() as u32;
        if measured == 0 {
            self.lock().status = failure.map_or(ScanStatus::Idle, |message| ScanStatus::Failed { message });
            return;
        }
        let outcome = match (failure, why) {
            (Some(message), _) => ScanOutcome::Failed { message },
            (None, None) => ScanOutcome::Complete,
            (None, Some(_)) => ScanOutcome::Stopped,
        };
        let found = analyse(&self.catalog, plan.first_hz, plan.step_hz, &samples, &times);
        let scan = Scan {
            summary: ScanSummary {
                version: FORMAT_VERSION,
                id: id.clone(),
                band_id: plan.band.id.clone(),
                band_label: plan.band.label.clone(),
                band_kind: plan.band.kind,
                first_hz: plan.first_hz,
                step_hz: plan.step_hz,
                points: plan.len() as u32,
                dwell_ms: plan.dwell_ms,
                mode: ctx.mode,
                started,
                finished: now_secs(),
                outcome,
                qth: ctx.qth,
                rig: ctx.rig,
                eibi: self.catalog.source_file().to_string(),
                noise_db: found.noise_db,
                measured,
                peaks: found.peaks,
                missed: found.missed,
            },
            samples,
        };
        if let Err(e) = self.save(&scan) {
            tracing::warn!("could not save scan {id}: {e}");
        }
        let sites = logged_sites(&self.catalog, &scan.summary.peaks);
        let mut inner = self.lock();
        inner.scans.insert(0, scan);
        inner.relog(&self.catalog);
        inner.status = ScanStatus::Done { id, sites };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::bands::BandKind;

    fn scan(id: &str, started: i64) -> Scan {
        Scan {
            summary: ScanSummary {
                version: FORMAT_VERSION,
                id: id.into(),
                band_id: "49m".into(),
                band_label: "49".into(),
                band_kind: BandKind::Broadcast,
                first_hz: 5_800_000,
                step_hz: 5_000,
                points: 2,
                dwell_ms: 300,
                mode: None,
                started,
                finished: started + 1,
                outcome: ScanOutcome::Complete,
                qth: Qth::default(),
                rig: "sim".into(),
                eibi: "sked-a26.csv".into(),
                noise_db: Some(-50),
                measured: 2,
                peaks: vec![],
                missed: vec![],
            },
            samples: vec![Some(-50), Some(-50)],
        }
    }

    #[test]
    fn files_load_newest_first_and_delete() {
        let dir = std::env::temp_dir().join(format!("swatlas-scans-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let svc = ScanService::new(Some(dir.clone()), crate::stations::bundled());
        for (id, at) in [("a", 10), ("b", 30), ("c", 20)] {
            svc.save(&scan(id, at)).unwrap();
        }
        std::fs::write(dir.join("broken.json"), "{").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let again = ScanService::new(Some(dir.clone()), crate::stations::bundled());
        let ids: Vec<String> = again.summaries().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["b", "c", "a"]);
        assert_eq!(again.scan("c").unwrap().samples.len(), 2);

        assert_eq!(again.delete("c"), Ok(true));
        assert_eq!(again.delete("c"), Ok(false));
        assert_eq!(again.delete("../swatlas.toml"), Ok(false));
        assert!(!dir.join("c.json").exists() && dir.join("a.json").exists());
        assert!(!dir.read_dir().unwrap().flatten().any(|e| e.path().to_string_lossy().ends_with(".tmp")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
