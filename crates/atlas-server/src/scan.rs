//! Band scans: running one on the rig a `ScanLink` lends, and the saved ones, one JSON file each
//! in `scans/`, with the logbook and the unidentified signals gathered from them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use atlas_core::api::Qth;
use atlas_core::scan::{
    analyse, analyse_so_far, logbook, logged_sites, on_air_sites, scan_id, unidentified, valid_id, Logged, Peak, Scan, ScanOutcome, ScanPing,
    ScanPlan, ScanStatus, ScanSummary, Signal, FORMAT_VERSION, PINGS_KEPT, READS, READ_GAP_MS,
};
use atlas_core::stations::{Catalog, CatalogCell};
use atlas_rig::{ScanLink, ScanStop};

use crate::files::write_atomic;

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
    catalog: Arc<CatalogCell>,
    inner: Mutex<Inner>,
}

struct Inner {
    status: ScanStatus,
    /// Newest first.
    scans: Vec<Scan>,
    /// Joined to the catalog of `generation`.
    logbook: Arc<Vec<Logged>>,
    generation: u32,
    signals: Arc<Vec<Signal>>,
    /// The running scan, with what it heard so far (in the logbook until it ends).
    live: Option<ScanSummary>,
}

impl Inner {
    fn relog(&mut self, catalog: &Catalog) {
        let all = || self.scans.iter().map(|s| &s.summary).chain(self.live.as_ref());
        self.logbook = Arc::new(logbook(all(), catalog));
        self.generation = catalog.generation();
        self.signals = Arc::new(unidentified(all()));
    }
}

/// Every scan file in `dir`, newest first; files that do not read are skipped.
fn load(dir: &Path) -> Vec<Scan> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut scans: Vec<Scan> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| {
            match std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("skipping scan {}: {e}", p.display());
                    None
                }
            }
        })
        .collect();
    scans
        .sort_by(|a: &Scan, b| b.summary.started.cmp(&a.summary.started).then_with(|| b.summary.id.cmp(&a.summary.id)));
    scans
}

impl ScanService {
    /// Load the saved scans and their logbook (against the running catalog).
    pub fn new(dir: Option<PathBuf>, catalog: Arc<CatalogCell>) -> Arc<Self> {
        let scans = dir.as_deref().map(load).unwrap_or_default();
        let mut inner = Inner {
            status: ScanStatus::Idle,
            scans,
            logbook: Default::default(),
            generation: 0,
            signals: Default::default(),
            live: None,
        };
        inner.relog(&catalog.get());
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

    /// One saved scan without its samples.
    pub fn summary(&self, id: &str) -> Option<ScanSummary> {
        self.lock().scans.iter().find(|s| s.summary.id == id).map(|s| s.summary.clone())
    }

    /// The stations the saved scans logged, as rows of the running catalog.
    pub fn logbook(&self) -> Arc<Vec<Logged>> {
        self.logbook_for(&self.catalog.get())
    }

    /// The logbook as rows of `cat` (the station list's Heard lens), joined again if the one kept
    /// is of another generation.
    pub fn logbook_for(&self, cat: &Catalog) -> Arc<Vec<Logged>> {
        let mut inner = self.lock();
        if inner.generation != cat.generation() {
            inner.relog(cat);
        }
        inner.logbook.clone()
    }

    /// Signals no station explained, across the saved scans.
    pub fn signals(&self) -> Arc<Vec<Signal>> {
        self.lock().signals.clone()
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
        inner.relog(&self.catalog.get());
        Ok(true)
    }

    /// Forget every saved scan and remove its file (other files in `scans/` stay). The scans
    /// whose file could not be removed are kept, and the first such error is returned.
    pub fn delete_all(&self) -> Result<u32, String> {
        let mut inner = self.lock();
        let mut failed = None;
        let before = inner.scans.len();
        inner.scans.retain(|s| {
            let Some(dir) = &self.dir else { return false };
            match std::fs::remove_file(dir.join(format!("{}.json", s.summary.id))) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    failed.get_or_insert(e.to_string());
                    true
                }
                _ => false,
            }
        });
        let deleted = (before - inner.scans.len()) as u32;
        inner.relog(&self.catalog.get());
        failed.map_or(Ok(deleted), Err)
    }

    /// Write `scan` to `scans/<id>.json`.
    fn save(&self, scan: &Scan) -> Result<(), String> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let json = serde_json::to_string(scan).expect("Scan serialises");
        write_atomic(&dir.join(format!("{}.json", scan.summary.id)), &json).map_err(|e| e.to_string())
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
            heard_rev: 0,
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
        // its record, filled in when it ends; meanwhile in the logbook with what it heard so far
        let draft = ScanSummary {
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
            finished: started,
            outcome: ScanOutcome::Complete,
            qth: ctx.qth,
            rig: ctx.rig,
            sources: vec![],
            noise_db: None,
            measured: 0,
            peaks: vec![],
            missed: vec![],
        };
        self.lock().live = Some(draft.clone());
        for i in 0..plan.len() {
            if link.stopped().is_some() {
                break;
            }
            if plan.covered[i] {
                match link.measure(plan.freq(i), dwell, READS, gap).await {
                    Ok(db) => {
                        times[i] = now_secs();
                        let db = db.map(|d| d.clamp(i16::MIN.into(), i16::MAX.into()) as i16);
                        let cat = self.catalog.get();
                        let sites = on_air_sites(&cat, plan.freq(i), plan.step_hz, times[i]);
                        let so_far = match &mut self.lock().status {
                            ScanStatus::Running { samples, pings, .. } => {
                                samples[i] = db;
                                if pings.len() == PINGS_KEPT {
                                    pings.remove(0);
                                }
                                pings.push(ScanPing { index: i as u32, db, sites });
                                samples.clone()
                            }
                            _ => continue,
                        };
                        let peaks = analyse_so_far(&cat, plan.first_hz, plan.step_hz, &so_far, &times, i + 1);
                        self.hear(&cat, peaks);
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
            let mut inner = self.lock();
            inner.live = None;
            inner.relog(&self.catalog.get());
            inner.status = failure.map_or(ScanStatus::Idle, |message| ScanStatus::Failed { message });
            return;
        }
        let outcome = match (failure, why) {
            (Some(message), _) => ScanOutcome::Failed { message },
            (None, None) => ScanOutcome::Complete,
            (None, Some(_)) => ScanOutcome::Stopped,
        };
        // one catalog for the matching, the logbook and the record of it
        let cat = self.catalog.get();
        let found = analyse(&cat, plan.first_hz, plan.step_hz, &samples, &times);
        let scan = Scan {
            summary: ScanSummary {
                finished: now_secs(),
                outcome,
                sources: cat.stamps(),
                noise_db: found.noise_db,
                measured,
                peaks: found.peaks,
                missed: found.missed,
                ..draft
            },
            samples,
        };
        if let Err(e) = self.save(&scan) {
            tracing::warn!("could not save scan {id}: {e}");
        }
        let sites = logged_sites(&cat, &scan.summary.peaks);
        let mut inner = self.lock();
        inner.live = None;
        inner.scans.insert(0, scan);
        inner.relog(&cat);
        inner.status = ScanStatus::Done { id, sites };
    }

    /// What the running scan heard so far: into the logbook, and told by the status, when it
    /// changed.
    fn hear(&self, cat: &Catalog, peaks: Vec<Peak>) {
        let mut inner = self.lock();
        let Some(live) = inner.live.as_mut() else { return };
        if live.peaks == peaks {
            return;
        }
        live.peaks = peaks;
        inner.relog(cat);
        if let ScanStatus::Running { heard_rev, .. } = &mut inner.status {
            *heard_rev += 1;
        }
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
                sources: vec![],
                noise_db: Some(-50),
                measured: 2,
                peaks: vec![],
                missed: vec![],
            },
            samples: vec![Some(-50), Some(-50)],
        }
    }

    /// A peak on the catalog's first station, heard at `at`.
    fn first_station_peak(cat: &Catalog, at: i64) -> Peak {
        use atlas_core::scan::{StationRef, Verdict};
        let st = cat.station(0).unwrap();
        let site = cat.site(st.site).unwrap();
        let r = StationRef {
            source: st.source.clone(),
            key: st.key.clone(),
            freq_hz: st.freq_hz,
            name: st.name.clone(),
            itu: st.itu.clone(),
            flag: st.flag.clone(),
            site_name: site.name.clone(),
            site_itu: site.itu.clone(),
            lang: String::new(),
            target: String::new(),
        };
        Peak { freq_hz: st.freq_hz, db: -20, at, verdict: Verdict::Sure, stations: vec![r] }
    }

    #[test]
    fn the_logbook_follows_the_running_catalog() {
        use atlas_core::rig::Mode;
        use atlas_core::source::{Record, SiteSpec, SourceData, SourceDesc, SourceKind};

        let cell = crate::sources::bundled_cell();
        let svc = ScanService::new(None, cell.clone());
        let cat = cell.get();
        let mut s = scan("a", 10);
        s.summary.peaks = vec![first_station_peak(&cat, 10)];
        {
            let mut inner = svc.lock();
            inner.scans.push(s);
            inner.relog(&cat);
        }
        assert_eq!(svc.logbook().iter().map(|l| l.station_id).collect::<Vec<_>>(), [0]);

        // a source with a station below every other: ids move up one, and the logbook with them
        let eibi = crate::sources::bundled();
        let early = SourceData {
            desc: SourceDesc {
                id: "test".into(),
                name: "Test".into(),
                kind: SourceKind::User,
                editable: true,
                url: None,
                version: "1".into(),
                season: None,
                report: None,
            },
            records: vec![Record {
                key: "x".into(),
                freq_hz: 1_000,
                mode: Mode::AM,
                name: "Early".into(),
                itu: "G".into(),
                site: SiteSpec::Known(0),
                slots: vec![],
                notes: String::new(),
            }],
        };
        cell.replace(Catalog::build(&[&eibi.data, &early], &eibi.vocabulary, &eibi.gazetteer));
        assert_eq!(svc.logbook().iter().map(|l| l.station_id).collect::<Vec<_>>(), [1]);
        // a snapshot of the old catalog still gets its own ids
        assert_eq!(svc.logbook_for(&cat).iter().map(|l| l.station_id).collect::<Vec<_>>(), [0]);
    }

    #[test]
    fn a_running_scan_is_in_the_logbook_as_it_hears() {
        let cell = crate::sources::bundled_cell();
        let svc = ScanService::new(None, cell.clone());
        let cat = cell.get();
        let rev = |svc: &ScanService| match svc.status() {
            ScanStatus::Running { heard_rev, .. } => heard_rev,
            s => panic!("{s:?}"),
        };
        {
            let mut inner = svc.lock();
            inner.live = Some(scan("live", 10).summary);
            inner.status = ScanStatus::Running {
                id: "live".into(),
                band_id: "49m".into(),
                first_hz: 5_800_000,
                step_hz: 5_000,
                started: 10,
                done: 0,
                total: 2,
                samples: vec![None; 2],
                pings: vec![],
                heard_rev: 0,
            };
        }
        // nothing new: no bump
        svc.hear(&cat, vec![]);
        assert_eq!(rev(&svc), 0);
        assert!(svc.logbook().is_empty());
        // a station: in the logbook, and the status says so (once)
        svc.hear(&cat, vec![first_station_peak(&cat, 12)]);
        svc.hear(&cat, vec![first_station_peak(&cat, 12)]);
        assert_eq!(rev(&svc), 1);
        assert_eq!(svc.logbook().iter().map(|l| (l.station_id, l.first)).collect::<Vec<_>>(), [(0, 12)]);
        // gone again when the scan ends without it
        {
            let mut inner = svc.lock();
            inner.live = None;
            inner.relog(&cat);
        }
        assert!(svc.logbook().is_empty());
    }

    #[test]
    fn files_load_newest_first_and_delete() {
        let dir = std::env::temp_dir().join(format!("swatlas-scans-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let svc = ScanService::new(Some(dir.clone()), crate::sources::bundled_cell());
        for (id, at) in [("a", 10), ("b", 30), ("c", 20)] {
            svc.save(&scan(id, at)).unwrap();
        }
        std::fs::write(dir.join("broken.json"), "{").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let again = ScanService::new(Some(dir.clone()), crate::sources::bundled_cell());
        let ids: Vec<String> = again.summaries().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["b", "c", "a"]);
        assert_eq!(again.scan("c").unwrap().samples.len(), 2);

        assert_eq!(again.delete("c"), Ok(true));
        assert_eq!(again.delete("c"), Ok(false));
        assert_eq!(again.delete("../swatlas.toml"), Ok(false));
        assert!(!dir.join("c.json").exists() && dir.join("a.json").exists());
        assert!(!dir.read_dir().unwrap().flatten().any(|e| e.path().to_string_lossy().ends_with(".tmp")));

        // all at once: only the scans' own files go
        assert_eq!(again.delete_all(), Ok(2));
        assert!(again.summaries().is_empty() && again.logbook().is_empty());
        assert!(!dir.join("a.json").exists() && !dir.join("b.json").exists());
        assert!(dir.join("broken.json").exists() && dir.join("notes.txt").exists());
        assert_eq!(again.delete_all(), Ok(0));
        let _ = std::fs::remove_dir_all(dir);
    }
}
