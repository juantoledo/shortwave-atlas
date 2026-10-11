//! Band scans: the channel grid a band is swept on, what a scan stores, what it found (signals
//! matched to the stations EiBi had on the air, and the stations it expected but did not hear),
//! the logbook: every station a scan has logged, joined to the running catalog, and the signals
//! no station explained, gathered across scans.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::Qth;
use crate::bands::{Band, BandKind};
use crate::calendar::{civil_from_days, SECS_PER_DAY};
use crate::rig::RxRange;
use crate::schedule::slot_on;
use crate::stations::{Catalog, Station, LOOKUP_TOLERANCE_HZ};

/// Most grid points one scan may visit (20 m at 1 kHz is 351; MW at 9 kHz is 120).
pub const MAX_STEPS: usize = 2000;
/// The step range a request may ask for.
pub const MIN_STEP_HZ: u32 = 500;
pub const MAX_STEP_HZ: u32 = 1_000_000;
/// How long to wait after tuning before reading the meter (the AGC settles), and its limits.
pub const DEFAULT_DWELL_MS: u32 = 300;
pub const MIN_DWELL_MS: u32 = 100;
pub const MAX_DWELL_MS: u32 = 2_000;
/// Meter reads per step, this far apart; the strongest counts (signals fade).
pub const READS: u32 = 3;
pub const READ_GAP_MS: u32 = 60;
/// A signal stands at least this far over the noise floor (one S-unit).
pub const SIGNAL_DB: i32 = 6;
/// `ScanSummary::version` of what this build writes.
pub const FORMAT_VERSION: u32 = 1;
/// Steps a running scan's status keeps in `pings` (the UI polls slower than a fast scan steps).
pub const PINGS_KEPT: usize = 8;

/// Steps a band's default is widened through when its own spacing would take too many.
const WIDER_STEPS_HZ: [u32; 7] = [1_000, 2_500, 5_000, 10_000, 12_500, 25_000, 100_000];

fn grid_points(lo_khz: u32, hi_khz: u32, step_hz: u32) -> usize {
    (u64::from(hi_khz.saturating_sub(lo_khz)) * 1000 / u64::from(step_hz.max(1)) + 1) as usize
}

/// The step a band is scanned at unless the listener picks another: its channel spacing (`base`;
/// 0 = not channelised: 1 kHz below 144 MHz, 12.5 kHz from there up), widened until the band
/// fits in `MAX_STEPS`.
pub fn default_step_hz(base: u32, lo_khz: u32, hi_khz: u32) -> u32 {
    let base = match base {
        0 if lo_khz >= 144_000 => 12_500,
        0 => 1_000,
        b => b,
    };
    let fits = |s: u32| grid_points(lo_khz, hi_khz, s) <= MAX_STEPS;
    if fits(base) {
        return base;
    }
    WIDER_STEPS_HZ.iter().copied().find(|&s| s > base && fits(s)).unwrap_or(WIDER_STEPS_HZ[WIDER_STEPS_HZ.len() - 1])
}

/// Where a band's channels sit when they are not on multiples of the step: CB channel 1
/// (26 965 kHz) and PMR446 channel 1 (446.00625 MHz). Returned modulo `step_hz`.
fn grid_offset_hz(band_id: &str, step_hz: u32) -> u32 {
    let anchor = match band_id {
        "cb" => 26_965_000,
        "pmr446" => 446_006_250,
        _ => 0,
    };
    anchor % step_hz
}

/// The frequencies one scan visits.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanPlan {
    pub band: Band,
    pub first_hz: u32,
    pub step_hz: u32,
    pub dwell_ms: u32,
    /// Per grid point: the rig receives it (otherwise it is skipped and stays unmeasured).
    pub covered: Vec<bool>,
}

impl ScanPlan {
    pub fn len(&self) -> usize {
        self.covered.len()
    }

    pub fn is_empty(&self) -> bool {
        self.covered.is_empty()
    }

    pub fn freq(&self, i: usize) -> u32 {
        self.first_hz + i as u32 * self.step_hz
    }
}

/// The grid for scanning `band` at `step_hz` (default: the band's), checked against the limits
/// and the rig's `coverage`.
pub fn plan(
    band: &Band,
    step_hz: Option<u32>,
    dwell_ms: Option<u32>,
    coverage: &[RxRange],
) -> Result<ScanPlan, String> {
    let step = step_hz.unwrap_or(band.scan_step_hz);
    if !(MIN_STEP_HZ..=MAX_STEP_HZ).contains(&step) {
        return Err(format!(
            "The step must be between {} and {} kHz",
            f64::from(MIN_STEP_HZ) / 1000.0,
            MAX_STEP_HZ / 1000
        ));
    }
    let dwell_ms = dwell_ms.unwrap_or(DEFAULT_DWELL_MS);
    if !(MIN_DWELL_MS..=MAX_DWELL_MS).contains(&dwell_ms) {
        return Err(format!("The dwell must be between {MIN_DWELL_MS} and {MAX_DWELL_MS} ms"));
    }
    let (lo, hi, step64) = (u64::from(band.lo_khz) * 1000, u64::from(band.hi_khz) * 1000, u64::from(step));
    let off = u64::from(grid_offset_hz(&band.id, step));
    let first = if lo <= off { off } else { off + (lo - off).div_ceil(step64) * step64 };
    if first > hi {
        return Err("The step is wider than the band".into());
    }
    let n = ((hi - first) / step64 + 1) as usize;
    if n > MAX_STEPS {
        return Err(format!("{n} steps is too many (at most {MAX_STEPS}): pick a larger step"));
    }
    let covered: Vec<bool> =
        (0..n as u64).map(|i| coverage.iter().any(|r| r.contains_hz((first + i * step64) as u32))).collect();
    if !covered.contains(&true) {
        return Err("The rig does not receive this band".into());
    }
    Ok(ScanPlan { band: band.clone(), first_hz: first as u32, step_hz: step, dwell_ms, covered })
}

/// Argument of `Call::StartScan`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScanRequest {
    /// `Band::id` of a band on the dial.
    pub band_id: String,
    /// `None` = the band's `scan_step_hz`.
    pub step_hz: Option<u32>,
    /// `None` = `DEFAULT_DWELL_MS`.
    pub dwell_ms: Option<u32>,
}

/// A station as a scan saw it. A copy rather than a `Station::id`, which only holds for one
/// version of the schedule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationRef {
    pub freq_hz: u32,
    pub name: String,
    /// The station's own country (EiBi code) and flag.
    pub itu: String,
    pub flag: Option<String>,
    pub site_name: String,
    pub site_itu: String,
    /// Language and target of the slot on the air then.
    pub lang: String,
    pub target: String,
}

/// What identifies a station across schedule versions.
type Key<'a> = (u32, &'a str, &'a str, &'a str, &'a str);

impl StationRef {
    pub(crate) fn of(cat: &Catalog, st: &Station, unix: i64) -> Self {
        let site = cat.site(st.site);
        let slot = st.slots.iter().find(|s| slot_on(s, unix));
        Self {
            freq_hz: st.freq_hz,
            name: st.name.clone(),
            itu: st.itu.clone(),
            flag: st.flag.clone(),
            site_name: site.map(|s| s.name.clone()).unwrap_or_default(),
            site_itu: site.map(|s| s.itu.clone()).unwrap_or_default(),
            lang: slot.map(|s| s.lang.clone()).unwrap_or_default(),
            target: slot.map(|s| s.target.clone()).unwrap_or_default(),
        }
    }

    fn key(&self) -> Key<'_> {
        (self.freq_hz, &self.name, &self.itu, &self.site_itu, &self.site_name)
    }

    /// The same station in `cat`, if this version of the schedule still has it.
    pub(crate) fn find<'c>(&self, cat: &'c Catalog) -> Option<&'c Station> {
        cat.between(self.freq_hz, self.freq_hz).find(|st| {
            st.name == self.name
                && st.itu == self.itu
                && cat.site(st.site).is_some_and(|s| s.itu == self.site_itu && s.name == self.site_name)
        })
    }

    /// The same station's key: frequency, name, country and site.
    pub(crate) fn same(&self, other: &StationRef) -> bool {
        self.key() == other.key()
    }
}

/// Who a signal is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Verdict {
    /// One station was on the air there.
    Sure,
    /// Several were: one of them.
    Ambiguous,
    /// Nothing on the air there in the schedule.
    Unidentified,
}

/// A signal the scan found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Peak {
    pub freq_hz: u32,
    /// dB relative to S9.
    pub db: i16,
    /// Unix seconds it was measured.
    #[ts(type = "number")]
    pub at: i64,
    pub verdict: Verdict,
    pub stations: Vec<StationRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[ts(export)]
pub enum ScanOutcome {
    Complete,
    /// Stopped, or the listener tuned away: what was measured is kept.
    Stopped,
    /// The rig stopped answering.
    Failed {
        message: String,
    },
}

/// A scan without its samples: what the history and the logbook need.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScanSummary {
    /// `FORMAT_VERSION` of the file.
    pub version: u32,
    /// `2026-10-10T143205Z-49m`: when it started and the band (also its file name).
    pub id: String,
    pub band_id: String,
    pub band_label: String,
    pub band_kind: BandKind,
    /// The grid: `points` frequencies from `first_hz`, `step_hz` apart.
    pub first_hz: u32,
    pub step_hz: u32,
    pub points: u32,
    pub dwell_ms: u32,
    /// The rig's mode during the scan.
    pub mode: Option<String>,
    /// Unix seconds.
    #[ts(type = "number")]
    pub started: i64,
    #[ts(type = "number")]
    pub finished: i64,
    pub outcome: ScanOutcome,
    pub qth: Qth,
    /// Which rig (backend, model).
    pub rig: String,
    /// The schedule file the signals were matched against (`sked-a26.csv`).
    pub eibi: String,
    /// The median reading, dB relative to S9.
    pub noise_db: Option<i16>,
    /// Grid points measured.
    pub measured: u32,
    pub peaks: Vec<Peak>,
    /// Stations on the air in the band that gave no signal where they were measured.
    pub missed: Vec<StationRef>,
}

impl ScanSummary {
    pub fn last_hz(&self) -> u32 {
        self.first_hz + self.points.saturating_sub(1) * self.step_hz
    }
}

/// One scan as saved: `scans/<id>.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Scan {
    pub summary: ScanSummary,
    /// dB relative to S9 per grid point; `None` where nothing was measured.
    pub samples: Vec<Option<i16>>,
}

/// Result of `Call::ScanStatus` and `Call::StartScan`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum ScanStatus {
    Idle,
    Running {
        id: String,
        band_id: String,
        first_hz: u32,
        step_hz: u32,
        /// Unix seconds.
        #[ts(type = "number")]
        started: i64,
        /// Grid points visited so far, of `total`.
        done: u32,
        total: u32,
        /// So far, as in `Scan::samples`.
        samples: Vec<Option<i16>>,
        /// The last `PINGS_KEPT` steps measured, oldest first (for the globe).
        pings: Vec<ScanPing>,
    },
    /// The last scan finished and was saved as `id`; `sites` are those of the stations it logged.
    Done {
        id: String,
        sites: Vec<u32>,
    },
    Failed {
        message: String,
    },
}

/// One measured step of a running scan: its reading and the transmitter sites on the air there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScanPing {
    /// Grid point, as in `Scan::samples`.
    pub index: u32,
    /// dB relative to S9.
    pub db: Option<i16>,
    /// `Site::id`s.
    pub sites: Vec<u32>,
}

/// How far from a grid point a station still belongs to it (as `analyse` matches peaks).
fn peak_tol(step_hz: u32) -> u32 {
    (step_hz / 2).max(LOOKUP_TOLERANCE_HZ)
}

/// The sites of stations on the air at `unix` within reach of a step at `freq_hz`, by id.
pub fn on_air_sites(cat: &Catalog, freq_hz: u32, step_hz: u32, unix: i64) -> Vec<u32> {
    let tol = peak_tol(step_hz);
    let sites: BTreeSet<u32> = cat
        .between(freq_hz.saturating_sub(tol), freq_hz.saturating_add(tol))
        .filter(|st| st.slots.iter().any(|s| slot_on(s, unix)))
        .map(|st| st.site)
        .collect();
    sites.into_iter().collect()
}

/// A station some scan logged, as it is in the running catalog.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Logged {
    pub station_id: u32,
    pub site_id: u32,
    /// Unix seconds of the first and the last time.
    #[ts(type = "number")]
    pub first: i64,
    #[ts(type = "number")]
    pub last: i64,
    /// Strongest signal, dB relative to S9.
    pub best_db: i16,
    /// Scans that logged it.
    pub count: u32,
    /// At least once it was the only station on the air there.
    pub sure: bool,
}

/// What a scan found in its samples.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub noise_db: Option<i16>,
    pub peaks: Vec<Peak>,
    pub missed: Vec<StationRef>,
}

/// The lower median of the readings.
pub fn noise_floor(samples: &[Option<i16>]) -> Option<i16> {
    let mut v: Vec<i16> = samples.iter().flatten().copied().collect();
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[(v.len() - 1) / 2])
}

/// Grid points that stand `SIGNAL_DB` over `floor` and above their neighbours (the first of a
/// flat top).
fn local_peaks(samples: &[Option<i16>], floor: i16) -> Vec<usize> {
    let thr = i32::from(floor) + SIGNAL_DB;
    (0..samples.len())
        .filter(|&i| {
            let Some(v) = samples[i] else { return false };
            let left = i.checked_sub(1).and_then(|j| samples[j]);
            let right = samples.get(i + 1).copied().flatten();
            i32::from(v) >= thr && left.is_none_or(|l| v > l) && right.is_none_or(|r| v >= r)
        })
        .collect()
}

/// Find the signals in a scan and say who they are. `times[i]` is when grid point `i` was
/// measured. A station on the air counts as logged where a peak is within half a step of it,
/// or where its own channel reads over the threshold (a weaker neighbour of a strong signal);
/// where its channel was measured without a signal it is missed.
pub fn analyse(cat: &Catalog, first_hz: u32, step_hz: u32, samples: &[Option<i16>], times: &[i64]) -> Found {
    let Some(floor) = noise_floor(samples) else { return Found::default() };
    let n = samples.len();
    let thr = i32::from(floor) + SIGNAL_DB;
    let freq = |i: usize| first_hz + i as u32 * step_hz;
    let at = |i: usize| times.get(i).copied().unwrap_or(0);
    let peak_tol = peak_tol(step_hz);
    let own_tol = (step_hz / 2).min(LOOKUP_TOLERANCE_HZ);

    let peaks = local_peaks(samples, floor);
    let mut hits: BTreeMap<usize, Vec<StationRef>> = peaks.iter().map(|&i| (i, vec![])).collect();
    let mut missed = vec![];
    let lo = first_hz.saturating_sub(peak_tol);
    let hi = freq(n - 1).saturating_add(peak_tol);
    for st in cat.between(lo, hi) {
        // the grid point nearest the station, and the peak nearest it
        let j = ((f64::from(st.freq_hz) - f64::from(first_hz)) / f64::from(step_hz)).round().clamp(0.0, (n - 1) as f64)
            as usize;
        if samples[j].is_none() || !st.slots.iter().any(|s| slot_on(s, at(j))) {
            continue;
        }
        let near_peak = peaks
            .iter()
            .copied()
            .filter(|&k| freq(k).abs_diff(st.freq_hz) <= peak_tol)
            .min_by_key(|&k| freq(k).abs_diff(st.freq_hz));
        let own =
            (freq(j).abs_diff(st.freq_hz) <= own_tol && samples[j].is_some_and(|v| i32::from(v) >= thr)).then_some(j);
        match near_peak.or(own) {
            Some(k) => hits.entry(k).or_default().push(StationRef::of(cat, st, at(k))),
            None => missed.push(StationRef::of(cat, st, at(j))),
        }
    }
    let peaks = hits
        .into_iter()
        .map(|(i, stations)| Peak {
            freq_hz: freq(i),
            db: samples[i].unwrap_or(floor),
            at: at(i),
            verdict: match stations.len() {
                0 => Verdict::Unidentified,
                1 => Verdict::Sure,
                _ => Verdict::Ambiguous,
            },
            stations,
        })
        .collect();
    Found { noise_db: Some(floor), peaks, missed }
}

/// The catalog's rows by `StationRef::key`: (station id, site id).
fn rows_by_key(cat: &Catalog) -> HashMap<Key<'_>, (u32, u32)> {
    cat.stations()
        .filter_map(|st| {
            let site = cat.site(st.site)?;
            Some((
                (st.freq_hz, st.name.as_str(), st.itu.as_str(), site.itu.as_str(), site.name.as_str()),
                (st.id, site.id),
            ))
        })
        .collect()
}

/// The sites of the stations `peaks` logged that `cat` still has, by id.
pub fn logged_sites(cat: &Catalog, peaks: &[Peak]) -> Vec<u32> {
    let ix = rows_by_key(cat);
    let sites: BTreeSet<u32> = peaks
        .iter()
        .filter(|p| p.verdict != Verdict::Unidentified)
        .flat_map(|p| &p.stations)
        .filter_map(|r| ix.get(&r.key()).map(|&(_, site)| site))
        .collect();
    sites.into_iter().collect()
}

/// Every station the scans logged, as rows of `cat` (stations the schedule no longer has are left
/// out), by station id.
pub fn logbook<'a>(scans: impl IntoIterator<Item = &'a ScanSummary>, cat: &Catalog) -> Vec<Logged> {
    let ix = rows_by_key(cat);
    let mut out: BTreeMap<u32, Logged> = BTreeMap::new();
    for scan in scans {
        for p in scan.peaks.iter().filter(|p| p.verdict != Verdict::Unidentified) {
            for r in &p.stations {
                let Some(&(station_id, site_id)) = ix.get(&r.key()) else { continue };
                let l = out.entry(station_id).or_insert(Logged {
                    station_id,
                    site_id,
                    first: p.at,
                    last: p.at,
                    best_db: p.db,
                    count: 0,
                    sure: false,
                });
                l.first = l.first.min(p.at);
                l.last = l.last.max(p.at);
                l.best_db = l.best_db.max(p.db);
                l.count += 1;
                l.sure |= p.verdict == Verdict::Sure;
            }
        }
    }
    out.into_values().collect()
}

/// A signal no station on the air explained, on one frequency across the scans that heard it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Signal {
    /// Where it was strongest.
    pub freq_hz: u32,
    /// Unix seconds of the first and the last time.
    #[ts(type = "number")]
    pub first: i64,
    #[ts(type = "number")]
    pub last: i64,
    /// dB relative to S9.
    pub best_db: i16,
    /// Scans that heard it.
    pub count: u32,
    /// The newest of those scans.
    pub last_scan: String,
}

/// The unidentified signals of `scans`, by frequency: peaks within `LOOKUP_TOLERANCE_HZ` of the
/// lowest of a run count as one signal.
pub fn unidentified<'a>(scans: impl IntoIterator<Item = &'a ScanSummary>) -> Vec<Signal> {
    let mut hits: Vec<(&Peak, &str)> = scans
        .into_iter()
        .flat_map(|s| s.peaks.iter().filter(|p| p.verdict == Verdict::Unidentified).map(move |p| (p, s.id.as_str())))
        .collect();
    hits.sort_by_key(|(p, _)| p.freq_hz);
    let mut out: Vec<Signal> = vec![];
    let mut start = 0;
    for (p, scan) in hits {
        match out.last_mut() {
            Some(s) if p.freq_hz - start <= LOOKUP_TOLERANCE_HZ => {
                if p.db > s.best_db {
                    s.best_db = p.db;
                    s.freq_hz = p.freq_hz;
                }
                s.first = s.first.min(p.at);
                if p.at >= s.last {
                    s.last = p.at;
                    s.last_scan = scan.to_string();
                }
                s.count += 1;
            }
            _ => {
                start = p.freq_hz;
                out.push(Signal {
                    freq_hz: p.freq_hz,
                    first: p.at,
                    last: p.at,
                    best_db: p.db,
                    count: 1,
                    last_scan: scan.to_string(),
                });
            }
        }
    }
    out
}

/// One scan's stations as rows of the running catalog: those it heard (with the reading) and
/// those it missed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScanMarks {
    pub heard: HashMap<u32, i16>,
    pub missed: HashSet<u32>,
}

impl ScanMarks {
    /// Every station id the scan mentions.
    pub fn ids(&self) -> HashSet<u32> {
        self.heard.keys().chain(&self.missed).copied().collect()
    }
}

/// What `scan` heard and missed, by station id in `cat` (stations it no longer has are left out).
pub fn scan_marks(cat: &Catalog, scan: &ScanSummary) -> ScanMarks {
    let mut m = ScanMarks::default();
    for p in scan.peaks.iter().filter(|p| p.verdict != Verdict::Unidentified) {
        for st in p.stations.iter().filter_map(|r| r.find(cat)) {
            let db = m.heard.entry(st.id).or_insert(p.db);
            *db = (*db).max(p.db);
        }
    }
    for st in scan.missed.iter().filter_map(|r| r.find(cat)) {
        if !m.heard.contains_key(&st.id) {
            m.missed.insert(st.id);
        }
    }
    m
}

/// A scan's id: when it started (UTC) and the band, `2026-10-10T143205Z-49m`.
pub fn scan_id(started: i64, band_id: &str) -> String {
    let (y, m, d) = civil_from_days(started.div_euclid(SECS_PER_DAY) as i32);
    let s = started.rem_euclid(SECS_PER_DAY);
    format!("{y:04}-{m:02}-{d:02}T{:02}{:02}{:02}Z-{band_id}", s / 3600, s / 60 % 60, s % 60)
}

/// An id that is safe as a file name (no separators, not hidden, not `..`).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('.')
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bands::{dial_bands, ItuRegion};
    use crate::calendar::days_from_civil;
    use crate::eibi::{parse, tests::files};
    use crate::rig::default_coverage;
    use crate::stations::{Lens, StationQuery};

    fn band(r: ItuRegion, id: &str) -> Band {
        dial_bands(r).into_iter().find(|b| b.id == id).unwrap()
    }

    fn catalog() -> Catalog {
        Catalog::from_eibi(parse(&files()).unwrap())
    }

    /// Thursday 2026-10-08 at `h:m` UTC.
    fn at(h: i64, m: i64) -> i64 {
        i64::from(days_from_civil(2026, 10, 8)) * SECS_PER_DAY + h * 3600 + m * 60
    }

    #[test]
    fn default_steps_follow_the_channel_plans() {
        use ItuRegion::{R1, R2, R3};
        let step = |r, id| band(r, id).scan_step_hz;
        assert_eq!(step(R1, "49m"), 5_000);
        assert_eq!(step(R2, "11m"), 5_000);
        assert_eq!((step(R1, "mw"), step(R3, "mw"), step(R2, "mw")), (9_000, 9_000, 10_000));
        assert_eq!(step(R1, "lw"), 9_000);
        assert_eq!((step(R2, "ham-40m"), step(R2, "ham-10m")), (1_000, 1_000));
        // 50-52 MHz at 1 kHz would be 2001 points, 50-54 MHz 4001
        assert_eq!((step(R1, "ham-4m"), step(R1, "ham-6m"), step(R2, "ham-6m")), (1_000, 2_500, 2_500));
        assert_eq!(
            (step(R2, "cb"), step(R2, "fm"), step(R2, "air"), step(R2, "marine")),
            (10_000, 100_000, 25_000, 25_000)
        );
        assert_eq!((step(R1, "ham-2m"), step(R1, "pmr446"), step(R2, "frs")), (12_500, 12_500, 25_000));
        assert_eq!(step(R2, "ham-33cm"), 25_000);
        // every band's default fits
        for r in [R1, R2, R3] {
            for b in dial_bands(r) {
                let p = plan(&b, None, None, &[RxRange { lo_khz: 0, hi_khz: 2_000_000 }]).unwrap();
                assert!(p.len() <= MAX_STEPS && p.len() > 1, "{} {r:?}: {}", b.id, p.len());
            }
        }
    }

    #[test]
    fn grids_sit_on_the_channels() {
        let all = default_coverage();
        let p = plan(&band(ItuRegion::R1, "49m"), None, None, &all).unwrap();
        assert_eq!((p.first_hz, p.len(), p.freq(p.len() - 1)), (5_800_000, 101, 6_300_000));
        // MW: 531 kHz and up in R1, 530 in R2
        let p = plan(&band(ItuRegion::R1, "mw"), None, None, &all).unwrap();
        assert_eq!((p.first_hz, p.freq(p.len() - 1)), (531_000, 1_602_000));
        let p = plan(&band(ItuRegion::R2, "mw"), None, None, &all).unwrap();
        assert_eq!((p.first_hz, p.freq(p.len() - 1)), (530_000, 1_700_000));
        // CB channel 1, and channel 40 at 27 405 kHz
        let p = plan(&band(ItuRegion::R2, "cb"), None, None, &all).unwrap();
        assert_eq!((p.first_hz, p.freq(p.len() - 1)), (26_965_000, 27_405_000));
        // an override keeps the anchor when it divides it
        let p = plan(&band(ItuRegion::R2, "cb"), Some(5_000), None, &all).unwrap();
        assert_eq!(p.first_hz, 26_965_000);
        let wide = [RxRange { lo_khz: 0, hi_khz: 1_000_000 }];
        let p = plan(&band(ItuRegion::R1, "pmr446"), None, None, &wide).unwrap();
        assert_eq!((p.first_hz, p.len()), (446_006_250, 16));
    }

    #[test]
    fn plans_are_checked() {
        let b49 = band(ItuRegion::R1, "49m");
        let all = default_coverage();
        assert!(plan(&b49, Some(100), None, &all).is_err());
        assert!(plan(&b49, Some(1_000), Some(50), &all).is_err());
        assert!(plan(&band(ItuRegion::R2, "ham-20m"), Some(500), None, &all).is_ok());
        let too_many = plan(&band(ItuRegion::R2, "ham-10m"), Some(500), None, &all).unwrap_err();
        assert!(too_many.contains("3401 steps"), "{too_many}");
        assert!(plan(&band(ItuRegion::R1, "ham-2m"), None, None, &all).is_err(), "past the coverage");
        // a band the rig receives in part: the rest is skipped
        let part = [RxRange { lo_khz: 5_900, hi_khz: 7_000 }];
        let p = plan(&b49, None, None, &part).unwrap();
        assert_eq!(p.covered.iter().filter(|c| **c).count(), 81);
        assert!(!p.covered[0] && p.covered[20]);
    }

    /// Readings on the 5 kHz grid from `first`, at `noise` dB except where given.
    fn profile(first: u32, n: usize, noise: i16, sig: &[(u32, i16)]) -> Vec<Option<i16>> {
        (0..n)
            .map(|i| {
                let f = first + i as u32 * 5_000;
                Some(sig.iter().find(|s| s.0 == f).map_or(noise, |s| s.1))
            })
            .collect()
    }

    #[test]
    fn floor_and_peaks() {
        assert_eq!(noise_floor(&[None, Some(-40), Some(-50), Some(-45), None]), Some(-45));
        assert_eq!(noise_floor(&[None]), None);
        // a lone peak, a flat top (its first point), one too weak, and one at the edge
        let s = [Some(-48), Some(-30), Some(-48), Some(-20), Some(-20), Some(-48), Some(-44), Some(-48), Some(-10)];
        assert_eq!(local_peaks(&s, -48), [1, 3, 8]);
        // unmeasured neighbours do not hide a peak
        assert_eq!(local_peaks(&[None, Some(0), None, Some(-48)], -48), [1]);
    }

    fn names(refs: &[StationRef]) -> Vec<(u32, &str)> {
        refs.iter().map(|r| (r.freq_hz / 1000, r.name.as_str())).collect()
    }

    #[test]
    fn signals_are_matched_to_the_schedule() {
        // 49 m at 16:30: CRI is on 5915 (16-17) and 6070 (24 h); Radio Bulgaria (5910, 14:30-15) is not
        let c = catalog();
        let (first, n) = (5_800_000, 101);
        let samples = profile(first, n, -48, &[(6_070_000, -6), (6_200_000, -12)]);
        let f = analyse(&c, first, 5_000, &samples, &vec![at(16, 30); n]);
        assert_eq!(f.noise_db, Some(-48));
        let got: Vec<(u32, i16, Verdict)> = f.peaks.iter().map(|p| (p.freq_hz / 1000, p.db, p.verdict)).collect();
        assert_eq!(got, [(6070, -6, Verdict::Sure), (6200, -12, Verdict::Unidentified)]);
        assert_eq!(names(&f.peaks[0].stations), [(6070, "China Radio Int.")]);
        assert_eq!(f.peaks[0].stations[0].lang, "M");
        assert_eq!(names(&f.missed), [(5915, "China Radio Int.")]);
    }

    #[test]
    fn a_weaker_neighbour_on_its_own_channel_is_logged() {
        // 14:45: Radio Bulgaria on 5910 under a strong unscheduled signal on 5905
        let c = catalog();
        let (first, n) = (5_800_000, 101);
        let samples = profile(first, n, -48, &[(5_905_000, -10), (5_910_000, -30), (6_070_000, -45)]);
        let f = analyse(&c, first, 5_000, &samples, &vec![at(14, 45); n]);
        let got: Vec<(u32, Verdict)> = f.peaks.iter().map(|p| (p.freq_hz / 1000, p.verdict)).collect();
        assert_eq!(got, [(5905, Verdict::Unidentified), (5910, Verdict::Sure)]);
        assert_eq!(names(&f.peaks[1].stations), [(5910, "Radio Bulgaria")]);
        // 6070 reads 3 dB over the floor: not a signal
        assert_eq!(names(&f.missed), [(6070, "China Radio Int.")]);
    }

    #[test]
    fn a_peak_off_the_grid_finds_its_station() {
        // 1 kHz steps through 6068-6072: the peak is a step off CRI's 6070
        let c = catalog();
        let samples = [Some(-50), Some(-40), Some(-20), Some(-25), Some(-50)];
        let f = analyse(&c, 6_067_000, 1_000, &samples, &[at(10, 0); 5]);
        assert_eq!(f.peaks.len(), 1);
        assert_eq!((f.peaks[0].freq_hz, f.peaks[0].verdict), (6_069_000, Verdict::Sure));
    }

    #[test]
    fn unmeasured_channels_are_not_missed() {
        let c = catalog();
        let n = 101;
        let samples = vec![None; n];
        assert_eq!(analyse(&c, 5_800_000, 5_000, &samples, &vec![at(10, 0); n]), Found::default());
        let mut half: Vec<Option<i16>> = vec![None; n];
        half[0] = Some(-50);
        half[1] = Some(-50);
        let f = analyse(&c, 5_800_000, 5_000, &half, &vec![at(10, 0); n]);
        assert!(f.peaks.is_empty() && f.missed.is_empty());
    }

    fn summary(peaks: Vec<Peak>) -> ScanSummary {
        ScanSummary {
            version: FORMAT_VERSION,
            id: scan_id(at(10, 0), "49m"),
            band_id: "49m".into(),
            band_label: "49".into(),
            band_kind: BandKind::Broadcast,
            first_hz: 5_800_000,
            step_hz: 5_000,
            points: 101,
            dwell_ms: DEFAULT_DWELL_MS,
            mode: Some("AM".into()),
            started: at(10, 0),
            finished: at(10, 1),
            outcome: ScanOutcome::Complete,
            qth: Qth::default(),
            rig: "sim".into(),
            eibi: "sked-a26.csv".into(),
            noise_db: Some(-48),
            measured: 101,
            peaks,
            missed: vec![],
        }
    }

    #[test]
    fn logbook_joins_the_catalog() {
        let c = catalog();
        let st = c.station(0).unwrap();
        let r = StationRef::of(&c, st, at(10, 0));
        let peak = |at: i64, db: i16, verdict: Verdict, stations: Vec<StationRef>| Peak {
            freq_hz: st.freq_hz,
            db,
            at,
            verdict,
            stations,
        };
        let mut gone = r.clone();
        gone.name = "Radio Nowhere".into();
        let scans = [
            summary(vec![peak(100, -20, Verdict::Ambiguous, vec![r.clone(), gone.clone()])]),
            summary(vec![peak(50, -6, Verdict::Sure, vec![r.clone()]), peak(70, 0, Verdict::Unidentified, vec![])]),
            summary(vec![peak(300, -30, Verdict::Ambiguous, vec![r.clone()])]),
        ];
        let log = logbook(&scans, &c);
        assert_eq!(
            log,
            [Logged { station_id: st.id, site_id: st.site, first: 50, last: 300, best_db: -6, count: 3, sure: true }]
        );
        assert!(!logbook(&scans[2..], &c)[0].sure);
        // the logged set filters the station list
        let ids = log.iter().map(|l| l.station_id).collect();
        let q = StationQuery { lens: Lens::Heard, facets: true, ..Default::default() };
        let page = c.search_in(&q, at(10, 0), Some(&ids));
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, st.id);
        assert_eq!(page.facets.unwrap().bands.values().sum::<u32>(), 1);
        assert_eq!(c.search_in(&q, at(10, 0), None).total, 0);
    }

    #[test]
    fn unidentified_signals_gather_across_scans() {
        let peak = |khz: u32, db: i16, at: i64| Peak {
            freq_hz: khz * 1000,
            db,
            at,
            verdict: Verdict::Unidentified,
            stations: vec![],
        };
        let mut a = summary(vec![peak(5905, -20, 100), peak(6200, -12, 100)]);
        a.id = "a".into();
        let mut b = summary(vec![peak(5906, -10, 300), peak(5950, -30, 300)]);
        b.id = "b".into();
        // a sure peak is not a signal of unknown origin
        b.peaks.push(Peak { verdict: Verdict::Sure, ..peak(6070, 0, 300) });
        let got = unidentified([&a, &b]);
        let brief: Vec<(u32, i16, u32, &str)> =
            got.iter().map(|s| (s.freq_hz / 1000, s.best_db, s.count, s.last_scan.as_str())).collect();
        assert_eq!(brief, [(5906, -10, 2, "b"), (5950, -30, 1, "b"), (6200, -12, 1, "a")]);
        assert_eq!((got[0].first, got[0].last), (100, 300));
        assert!(unidentified([]).is_empty());
    }

    #[test]
    fn a_scan_marks_its_stations_in_the_catalog() {
        let c = catalog();
        let (a, b) = (c.station(0).unwrap(), c.station(3).unwrap());
        let r = |st: &Station| StationRef::of(&c, st, at(10, 0));
        let mut gone = r(b);
        gone.site_name = "Nowhere".into();
        let mut s = summary(vec![
            Peak { freq_hz: a.freq_hz, db: -20, at: 0, verdict: Verdict::Sure, stations: vec![r(a)] },
            Peak { freq_hz: a.freq_hz, db: -8, at: 0, verdict: Verdict::Ambiguous, stations: vec![r(a), gone.clone()] },
        ]);
        s.missed = vec![r(b), gone];
        let m = scan_marks(&c, &s);
        assert_eq!(m.heard, HashMap::from([(a.id, -8)]));
        assert_eq!(m.missed, HashSet::from([b.id]));
        assert_eq!(m.ids(), HashSet::from([a.id, b.id]));
        assert_eq!(r(a).find(&c).map(|st| st.id), Some(a.id));
    }

    #[test]
    fn sites_on_the_air_at_a_step() {
        let c = catalog();
        let cnr = c.between(6_070_000, 6_070_000).next().unwrap();
        // 6070 is on all day; 5915 only 16-17 UTC
        assert_eq!(on_air_sites(&c, 6_070_000, 5_000, at(10, 0)), [cnr.site]);
        assert_eq!(on_air_sites(&c, 6_071_000, 1_000, at(10, 0)), [cnr.site], "within 1 kHz");
        assert!(on_air_sites(&c, 6_072_000, 1_000, at(10, 0)).is_empty());
        assert!(on_air_sites(&c, 5_915_000, 5_000, at(10, 0)).is_empty());
        assert_eq!(on_air_sites(&c, 5_915_000, 5_000, at(16, 30)).len(), 1);
    }

    #[test]
    fn sites_a_scan_logged() {
        let c = catalog();
        let (a, b) = (c.station(0).unwrap(), c.station(3).unwrap());
        let r = |st: &Station| StationRef::of(&c, st, at(10, 0));
        let peak = |verdict, stations| Peak { freq_hz: 0, db: 0, at: 0, verdict, stations };
        let peaks = [
            peak(Verdict::Sure, vec![r(a)]),
            peak(Verdict::Ambiguous, vec![r(a), r(b)]),
            peak(Verdict::Unidentified, vec![]),
        ];
        let mut want = vec![a.site, b.site];
        want.sort_unstable();
        want.dedup();
        assert_eq!(logged_sites(&c, &peaks), want);
        assert!(logged_sites(&c, &peaks[2..]).is_empty());
    }

    #[test]
    fn ids_and_json() {
        let id =
            scan_id(i64::from(days_from_civil(2026, 10, 10)) * SECS_PER_DAY + 14 * 3600 + 32 * 60 + 5, "ham-1.25m");
        assert_eq!(id, "2026-10-10T143205Z-ham-1.25m");
        assert!(valid_id(&id));
        for bad in ["", "..", ".hidden", "a/b", "a\\b", "c:x", "x y"] {
            assert!(!valid_id(bad), "{bad}");
        }
        let scan = Scan { summary: summary(vec![]), samples: vec![Some(-48), None, Some(3)] };
        let back: Scan = serde_json::from_str(&serde_json::to_string(&scan).unwrap()).unwrap();
        assert_eq!(back, scan);
        let j = serde_json::to_value(ScanStatus::Done { id: "x".into(), sites: vec![3] }).unwrap();
        assert_eq!((j["state"].as_str(), j["id"].as_str(), j["sites"][0].as_u64()), (Some("done"), Some("x"), Some(3)));
        let j = serde_json::to_value(ScanOutcome::Failed { message: "m".into() }).unwrap();
        assert_eq!(j["kind"], "failed");
    }
}
