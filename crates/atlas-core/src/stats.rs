//! What the listening history says, for one period: totals against the period before, a
//! timeline, the week's hours, breakdowns by station, country, language, target, band and mode,
//! distances, and what the paths were like. Periods, streaks and months follow the listener's
//! local day (`tz_offset_min`); hours of the day are UTC, like the schedules.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::calendar::{civil_from_days, day_of, days_from_civil, weekday};
use crate::collect::{collection, langs_of, Collection};
use crate::geo::SunPhase;
use crate::listening::Session;
use crate::scan::StationRef;
use crate::stations::Catalog;

const DAY_S: i64 = 86_400;
/// Rows in a breakdown before "other".
pub const TOP: usize = 10;
/// Sessions in `recent`, and grey-line catches listed.
pub const RECENT: usize = 20;
/// Bands need this many sessions in an hour to be "best" in it.
pub const BEST_MIN_SESSIONS: u32 = 3;

/// The listener's day number (days since 1970-01-01, local).
pub fn local_day(unix: i64, tz_min: i32) -> i32 {
    (unix + i64::from(tz_min) * 60).div_euclid(DAY_S) as i32
}

/// Unix time of the local midnight that starts `day`.
pub fn day_start(day: i32, tz_min: i32) -> i64 {
    i64::from(day) * DAY_S - i64::from(tz_min) * 60
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Period {
    /// Today.
    Day,
    /// The last 7 days, today included.
    #[default]
    Week,
    /// The last 30 days.
    Month,
    /// The last 365 days.
    Year,
    All,
}

impl Period {
    fn days(self) -> Option<i32> {
        match self {
            Period::Day => Some(1),
            Period::Week => Some(7),
            Period::Month => Some(30),
            Period::Year => Some(365),
            Period::All => None,
        }
    }
}

/// Argument of `Call::Stats`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatsQuery {
    pub period: Period,
    /// Minutes east of UTC (Lisbon in summer: 60; Santiago in winter: -240).
    #[serde(default)]
    pub tz_offset_min: i32,
}

/// The session under way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NowListening {
    pub session: Session,
    /// The station's row in the running catalog.
    pub station_id: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Totals {
    pub seconds: u32,
    pub sessions: u32,
    pub stations: u32,
    pub countries: u32,
    pub languages: u32,
    /// Seconds, within the period.
    pub longest: u32,
    pub average: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Bucket {
    #[ts(type = "number")]
    pub start: i64,
    pub seconds: u32,
}

/// One row of a breakdown. `key` is the code (country, language, band, ...), the station's
/// identity for stations (empty: unidentified), or `*` for the rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Share {
    pub key: String,
    pub seconds: u32,
    pub sessions: u32,
    /// Against the period before.
    pub delta_s: i32,
    /// For stations.
    pub station: Option<StationRef>,
    pub station_id: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Breakdowns {
    pub stations: Vec<Share>,
    pub countries: Vec<Share>,
    pub languages: Vec<Share>,
    pub targets: Vec<Share>,
    pub bands: Vec<Share>,
    pub modes: Vec<Share>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DistBin {
    pub lo_km: u32,
    /// `None`: and beyond.
    pub hi_km: Option<u32>,
    pub seconds: u32,
    pub sessions: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SiteTime {
    pub site: u32,
    pub seconds: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Streak {
    /// Days in a row up to today (or yesterday, if nothing yet today).
    pub current_days: u32,
    pub best_days: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum MilestoneKind {
    FirstCountry { itu: String },
    Countries { n: u32 },
    Stations { n: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Milestone {
    #[ts(type = "number")]
    pub at: i64,
    pub what: MilestoneKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BandDb {
    pub band: String,
    pub db: i16,
    pub sessions: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BandSplit {
    pub band: String,
    pub day_s: u32,
    pub twilight_s: u32,
    pub night_s: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Propagation {
    /// Sessions with either end in twilight: how many, and the latest.
    pub grey_count: u32,
    pub grey: Vec<Session>,
    /// Per UTC hour (24): the band with the best mean S-meter, given enough sessions.
    pub best_by_hour: Vec<Option<BandDb>>,
    /// Time per band by the Sun at the listener, most listened first.
    pub day_night: Vec<BandSplit>,
    /// The weakest identified signals listened to for two minutes or more.
    pub faint: Vec<Session>,
}

/// How many sessions each clean-up would remove.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClearCounts {
    pub all: u32,
    pub older_30: u32,
    pub older_90: u32,
    pub older_365: u32,
}

/// Result of `Call::Stats`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Stats {
    pub period: Period,
    /// Where the period starts.
    #[ts(type = "number")]
    pub from: i64,
    pub now: Option<NowListening>,
    pub totals: Totals,
    /// The same span one period earlier, up to the same time (none for All).
    pub previous: Option<Totals>,
    pub timeline: Vec<Bucket>,
    /// Weekday (Monday first) × UTC hour, seconds.
    pub heat: Vec<Vec<u32>>,
    pub by: Breakdowns,
    pub distance: Vec<DistBin>,
    pub farthest: Option<Session>,
    /// Newest first.
    pub recent: Vec<Session>,
    /// Transmitter sites listened to, by time (for the globe).
    pub sites: Vec<SiteTime>,
    pub propagation: Propagation,
    /// All time, whatever the period.
    pub streak: Streak,
    pub milestones: Vec<Milestone>,
    pub collection: Collection,
    pub clear: ClearCounts,
}

fn overlap(s: &Session, a: i64, b: i64) -> i64 {
    (s.end.min(b) - s.start.max(a)).max(0)
}

fn sat(v: i64) -> u32 {
    v.clamp(0, i64::from(u32::MAX)) as u32
}

/// The local month (`year`, `month`) `back` months before the one of `day`.
fn month_back(day: i32, back: i32) -> (i32, u32) {
    let (y, m, _) = civil_from_days(day);
    let idx = y * 12 + m as i32 - 1 - back;
    (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32)
}

fn timeline_edges(period: Period, from: i64, now: i64, first: Option<i64>, tz: i32) -> Vec<i64> {
    let today = local_day(now, tz);
    let months = |n: i32| -> Vec<i64> {
        (0..=n)
            .rev()
            .map(|b| {
                let (y, m) = month_back(today, b);
                day_start(days_from_civil(y, m, 1), tz)
            })
            .collect()
    };
    let mut edges: Vec<i64> = match period {
        Period::Day => (0..24).map(|h| from + h * 3600).collect(),
        Period::Week | Period::Month => (0..period.days().unwrap_or(0)).map(|d| from + i64::from(d) * DAY_S).collect(),
        Period::Year => months(11),
        Period::All => {
            let Some(first) = first else { return vec![] };
            let (fy, fm, _) = civil_from_days(local_day(first, tz));
            let (ty, tm, _) = civil_from_days(today);
            let span = (ty * 12 + tm as i32) - (fy * 12 + fm as i32);
            if span <= 36 {
                months(span)
            } else {
                (fy..=ty).map(|y| day_start(days_from_civil(y, 1, 1), tz)).collect()
            }
        }
    };
    edges.push(now.max(edges.last().copied().unwrap_or(now)));
    edges
}

/// Per key: seconds, sessions, and the station (for stations).
type Tallied = (i64, u32, Option<StationRef>);
type Tally = HashMap<String, Tallied>;

fn tally<'a>(list: &[&'a Session], a: i64, b: i64, keys: impl Fn(&'a Session) -> Vec<String>) -> Tally {
    let mut t: Tally = HashMap::new();
    for s in list {
        let secs = overlap(s, a, b);
        if secs == 0 {
            continue;
        }
        for k in keys(s) {
            let e = t.entry(k).or_insert((0, 0, None));
            e.0 += secs;
            e.1 += 1;
            if s.station.is_some() {
                e.2.clone_from(&s.station);
            }
        }
    }
    t
}

fn shares(now: Tally, before: &Tally, cat: &Catalog, stations: bool) -> Vec<Share> {
    let mut rows: Vec<(String, Tallied)> = now.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    let mut out: Vec<Share> = vec![];
    let mut other = (0i64, 0u32);
    for (i, (key, (secs, n, st))) in rows.into_iter().enumerate() {
        if i >= TOP {
            other.0 += secs;
            other.1 += n;
            continue;
        }
        let prev = before.get(&key).map_or(0, |p| p.0);
        let station_id = st.as_ref().and_then(|r| r.find(cat)).map(|s| s.id);
        out.push(Share {
            key,
            seconds: sat(secs),
            sessions: n,
            delta_s: (secs - prev).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            station: if stations { st } else { None },
            station_id: if stations { station_id } else { None },
        });
    }
    if other.0 > 0 {
        out.push(Share { key: "*".into(), seconds: sat(other.0), sessions: other.1, delta_s: 0, station: None, station_id: None });
    }
    out
}

fn totals(list: &[&Session], a: i64, b: i64) -> Totals {
    let mut t = Totals::default();
    let (mut st, mut co, mut la) = (HashSet::new(), HashSet::new(), HashSet::new());
    let mut secs = 0i64;
    for s in list {
        let o = overlap(s, a, b);
        if o == 0 {
            continue;
        }
        secs += o;
        t.sessions += 1;
        t.longest = t.longest.max(sat(o));
        if let Some(r) = &s.station {
            st.insert(r.ident());
            la.extend(langs_of(s));
        }
        if let Some(c) = s.country() {
            co.insert(c);
        }
    }
    t.seconds = sat(secs);
    t.stations = st.len() as u32;
    t.countries = co.len() as u32;
    t.languages = la.len() as u32;
    t.average = if t.sessions > 0 { t.seconds / t.sessions } else { 0 };
    t
}

fn streak(all: &[&Session], now: i64, tz: i32) -> Streak {
    let mut days: Vec<i32> =
        all.iter().flat_map(|s| local_day(s.start, tz)..=local_day((s.end - 1).max(s.start), tz)).collect();
    days.sort_unstable();
    days.dedup();
    let (mut best, mut run, mut last) = (0u32, 0u32, None::<i32>);
    for &d in &days {
        run = if last == Some(d - 1) { run + 1 } else { 1 };
        best = best.max(run);
        last = Some(d);
    }
    let today = local_day(now, tz);
    let current = if last == Some(today) || last == Some(today - 1) { run } else { 0 };
    Streak { current_days: current, best_days: best }
}

fn milestones(all: &[&Session]) -> Vec<Milestone> {
    const COUNTRIES: [u32; 4] = [10, 25, 50, 100];
    const STATIONS: [u32; 5] = [10, 50, 100, 250, 500];
    let (mut countries, mut stations) = (HashSet::new(), HashSet::new());
    let mut out = vec![];
    for s in all {
        let Some(r) = &s.station else { continue };
        if stations.insert(r.ident()) && STATIONS.contains(&(stations.len() as u32)) {
            out.push(Milestone { at: s.start, what: MilestoneKind::Stations { n: stations.len() as u32 } });
        }
        if let Some(c) = s.country() {
            if countries.insert(c.to_string()) {
                out.push(Milestone { at: s.start, what: MilestoneKind::FirstCountry { itu: c.into() } });
                if COUNTRIES.contains(&(countries.len() as u32)) {
                    out.push(Milestone { at: s.start, what: MilestoneKind::Countries { n: countries.len() as u32 } });
                }
            }
        }
    }
    out.reverse();
    out.truncate(50);
    out
}

fn propagation(list: &[&Session], a: i64, b: i64) -> Propagation {
    let id: Vec<&Session> = list.iter().copied().filter(|s| s.station.is_some() && overlap(s, a, b) > 0).collect();
    let grey: Vec<&Session> = id.iter().copied().filter(|s| s.sky.is_some_and(|k| k.grey_line())).collect();
    let mut cells: HashMap<(usize, &str), (i64, u32)> = HashMap::new();
    let mut split: HashMap<&str, [i64; 3]> = HashMap::new();
    for s in &id {
        if s.band == crate::eibi::names::OUT_OF_BAND {
            continue;
        }
        if let Some(db) = s.avg_db {
            let hour = (s.start.rem_euclid(DAY_S) / 3600) as usize;
            let c = cells.entry((hour, &s.band)).or_insert((0, 0));
            c.0 += i64::from(db);
            c.1 += 1;
        }
        if let Some(k) = s.sky {
            let i = match k.rx {
                SunPhase::Day => 0,
                SunPhase::Twilight => 1,
                SunPhase::Night => 2,
            };
            split.entry(&s.band).or_default()[i] += overlap(s, a, b);
        }
    }
    let best_by_hour = (0..24)
        .map(|h| {
            cells
                .iter()
                .filter(|((hh, _), (_, n))| *hh == h && *n >= BEST_MIN_SESSIONS)
                .map(|((_, band), (sum, n))| BandDb { band: band.to_string(), db: (*sum as f64 / f64::from(*n)).round() as i16, sessions: *n })
                .max_by(|x, y| x.db.cmp(&y.db).then_with(|| y.band.cmp(&x.band)))
        })
        .collect();
    let mut day_night: Vec<BandSplit> = split
        .into_iter()
        .map(|(band, [d, t, n])| BandSplit { band: band.into(), day_s: sat(d), twilight_s: sat(t), night_s: sat(n) })
        .collect();
    day_night.sort_by(|x, y| {
        let tot = |s: &BandSplit| u64::from(s.day_s) + u64::from(s.twilight_s) + u64::from(s.night_s);
        tot(y).cmp(&tot(x)).then_with(|| x.band.cmp(&y.band))
    });
    let mut faint: Vec<&Session> =
        id.iter().copied().filter(|s| s.secs() >= 120 && s.avg_db.is_some()).collect();
    faint.sort_by_key(|s| (s.avg_db, s.start));
    faint.truncate(5);
    Propagation {
        grey_count: grey.len() as u32,
        grey: grey.iter().rev().take(RECENT).map(|s| (*s).clone()).collect(),
        best_by_hour,
        day_night,
        faint: faint.into_iter().cloned().collect(),
    }
}

/// The stats for `q` at `now`: `closed` sessions oldest first, and the one under way.
pub fn stats(closed: &[Session], open: Option<&Session>, q: &StatsQuery, now: i64, cat: &Catalog) -> Stats {
    let tz = q.tz_offset_min;
    let all: Vec<&Session> = closed.iter().chain(open).collect();
    let today = local_day(now, tz);
    let first = all.first().map(|s| s.start);
    let (from, prev) = match q.period.days() {
        Some(n) => {
            let from = day_start(today - (n - 1), tz);
            // the same span, n days earlier: this morning against yesterday morning
            let back = i64::from(n) * DAY_S;
            (from, Some((from - back, now - back)))
        }
        None => (first.unwrap_or(now).min(now), None),
    };
    let in_window: Vec<&Session> = all.iter().copied().filter(|s| overlap(s, from, now) > 0).collect();
    let before: Vec<&Session> = match prev {
        Some((a, b)) => all.iter().copied().filter(|s| overlap(s, a, b) > 0).collect(),
        None => vec![],
    };
    let (pa, pb) = prev.unwrap_or((0, 0));

    let edges = timeline_edges(q.period, from, now, first, tz);
    let timeline = edges
        .windows(2)
        .map(|w| Bucket { start: w[0], seconds: sat(in_window.iter().map(|s| overlap(s, w[0], w[1])).sum()) })
        .collect();

    let mut heat = vec![vec![0u32; 24]; 7];
    for s in &in_window {
        let mut t = s.start.max(from);
        let end = s.end.min(now);
        while t < end {
            let next = ((t.div_euclid(3600) + 1) * 3600).min(end);
            let wd = weekday(day_of(t)) as usize;
            let h = (t.rem_euclid(DAY_S) / 3600) as usize;
            heat[wd][h] = heat[wd][h].saturating_add(sat(next - t));
            t = next;
        }
    }

    let station_key = |s: &Session| vec![s.station.as_ref().map(StationRef::ident).unwrap_or_default()];
    let country_key = |s: &Session| s.country().map(String::from).into_iter().collect::<Vec<_>>();
    let lang_key = |s: &Session| langs_of(s).into_iter().map(String::from).collect::<Vec<_>>();
    let target_key =
        |s: &Session| s.station.as_ref().map(|r| r.target.clone()).filter(|t| !t.is_empty()).into_iter().collect();
    let band_key = |s: &Session| vec![s.band.clone()];
    let mode_key = |s: &Session| vec![s.mode.clone().unwrap_or_else(|| "?".into())];
    macro_rules! by {
        ($k:expr, $st:expr) => {
            shares(tally(&in_window, from, now, $k), &tally(&before, pa, pb, $k), cat, $st)
        };
    }
    let by = Breakdowns {
        stations: by!(station_key, true),
        countries: by!(country_key, false),
        languages: by!(lang_key, false),
        targets: by!(target_key, false),
        bands: by!(band_key, false),
        modes: by!(mode_key, false),
    };

    const BINS: [(u32, Option<u32>); 5] =
        [(0, Some(1_000)), (1_000, Some(3_000)), (3_000, Some(6_000)), (6_000, Some(10_000)), (10_000, None)];
    let distance = BINS
        .iter()
        .map(|&(lo, hi)| {
            let inside: Vec<&&Session> = in_window
                .iter()
                .filter(|s| s.km.is_some_and(|k| k >= lo as f32 && hi.is_none_or(|h| k < h as f32)))
                .collect();
            DistBin {
                lo_km: lo,
                hi_km: hi,
                seconds: sat(inside.iter().map(|s| overlap(s, from, now)).sum()),
                sessions: inside.len() as u32,
            }
        })
        .collect();
    let farthest = in_window
        .iter()
        .filter(|s| s.station.is_some() && s.km.is_some())
        .max_by(|a, b| a.km.unwrap_or(0.0).total_cmp(&b.km.unwrap_or(0.0)))
        .map(|s| (*s).clone());
    let open_id = open.map(|o| o.id.as_str());
    let recent = in_window.iter().rev().filter(|s| Some(s.id.as_str()) != open_id).take(RECENT).map(|s| (*s).clone()).collect();

    let mut site_secs: HashMap<u32, i64> = HashMap::new();
    for s in &in_window {
        if let Some(st) = s.station.as_ref().and_then(|r| r.find(cat)) {
            *site_secs.entry(st.site).or_default() += overlap(s, from, now);
        }
    }
    let mut sites: Vec<SiteTime> = site_secs.into_iter().map(|(site, s)| SiteTime { site, seconds: sat(s) }).collect();
    sites.sort_by(|a, b| b.seconds.cmp(&a.seconds).then(a.site.cmp(&b.site)));

    let ago = |d: i64| closed.iter().filter(|s| s.end < now - d * DAY_S).count() as u32;
    Stats {
        period: q.period,
        from,
        now: open.map(|s| NowListening {
            session: s.clone(),
            station_id: s.station.as_ref().and_then(|r| r.find(cat)).map(|st| st.id),
        }),
        totals: totals(&in_window, from, now),
        previous: prev.map(|(a, b)| totals(&before, a, b)),
        timeline,
        heat,
        by,
        distance,
        farthest,
        recent,
        sites,
        propagation: propagation(&in_window, from, now),
        streak: streak(&all, now, tz),
        milestones: milestones(&all),
        collection: collection(&all, now, tz),
        clear: ClearCounts { all: closed.len() as u32, older_30: ago(30), older_90: ago(90), older_365: ago(365) },
    }
}

/// The station ids of the running catalog the history has sessions of (the Listened lens).
pub fn listened_ids<'a>(all: impl IntoIterator<Item = &'a Session>, cat: &Catalog) -> HashSet<u32> {
    let mut seen = HashSet::new();
    all.into_iter()
        .filter_map(|s| s.station.as_ref())
        .filter(|r| seen.insert(r.ident()))
        .filter_map(|r| r.find(cat))
        .map(|s| s.id)
        .collect()
}

/// One week of a station's signal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrendPoint {
    #[ts(type = "number")]
    pub start: i64,
    /// Mean of the sessions' mean S-meter, dB over S9.
    pub db: Option<i16>,
    pub sessions: u32,
}

/// Weeks in a station trend.
pub const TREND_WEEKS: i64 = 26;

/// The station's signal week by week, the last `TREND_WEEKS` up to `now`.
pub fn trend(all: &[Session], station: &StationRef, now: i64) -> Vec<TrendPoint> {
    let week = 7 * DAY_S;
    let from = now - TREND_WEEKS * week;
    let mut acc = vec![(0i64, 0u32); TREND_WEEKS as usize];
    for s in all.iter().filter(|s| s.start >= from && s.station.as_ref().is_some_and(|r| r.same(station))) {
        let i = (((s.start - from) / week) as usize).min(acc.len() - 1);
        if let Some(db) = s.avg_db {
            acc[i].0 += i64::from(db);
            acc[i].1 += 1;
        }
    }
    acc.iter()
        .enumerate()
        .map(|(i, &(sum, n))| TrendPoint {
            start: from + i as i64 * week,
            db: (n > 0).then(|| (sum as f64 / f64::from(n)).round() as i16),
            sessions: n,
        })
        .collect()
}

/// `trend` for the catalog's station `key`; empty if the catalog has no such station.
pub fn trend_for(all: &[Session], cat: &Catalog, key: &crate::source::StationKey, now: i64) -> Vec<TrendPoint> {
    match cat.by_key(&key.source, &key.key) {
        Some(st) => trend(all, &StationRef::of(cat, st, now), now),
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::tests::{sess, sref};
    use crate::listening::tests::{catalog, T0};
    use crate::listening::Sky;

    fn q(period: Period, tz: i32) -> StatsQuery {
        StatsQuery { period, tz_offset_min: tz }
    }

    #[test]
    fn local_days() {
        assert_eq!(local_day(T0, 0), local_day(T0 + DAY_S - 1, 0));
        assert_eq!(local_day(T0 - 1, 0), local_day(T0, 0) - 1);
        // 23:00 UTC is tomorrow at UTC+2
        assert_eq!(local_day(T0 + 23 * 3600, 120), local_day(T0, 0) + 1);
        assert_eq!(day_start(local_day(T0, 0), 0), T0);
        assert_eq!(day_start(local_day(T0, -240), -240), T0 - DAY_S + 4 * 3600);
    }

    #[test]
    fn periods_and_the_one_before() {
        let cat = catalog();
        let cri = || Some(sref("CRI", "CHN", "M"));
        let rnz = || Some(sref("RNZ", "NZL", "E,M"));
        let closed = vec![
            sess(T0 - 8 * DAY_S, 60, cri()),       // the week before
            sess(T0 - 2 * DAY_S, 30, rnz()),       // this week
            sess(T0 - 3600, 120, cri()),           // across midnight: 60 min yesterday, 60 today
            sess(T0 + 2 * 3600, 10, None),         // today, unidentified
        ];
        let now = T0 + 3 * 3600;
        let open = sess(T0 + 2 * 3600 + 1800, 30, cri()); // under way, 30 min so far
        let day = stats(&closed, Some(&open), &q(Period::Day, 0), now, &cat);
        assert_eq!(day.from, T0);
        assert_eq!((day.totals.seconds, day.totals.sessions, day.totals.stations), (100 * 60, 3, 1));
        assert_eq!(day.totals.longest, 3600);
        // yesterday up to 03:00 only, not all of yesterday
        assert_eq!(day.previous.as_ref().unwrap().seconds, 0);
        assert_eq!(day.timeline.len(), 24);
        assert_eq!(day.timeline[0].seconds, 3600);
        assert_eq!(day.now.as_ref().unwrap().session.id, open.id);
        assert!(day.recent.iter().all(|s| s.id != open.id));

        let week = stats(&closed, Some(&open), &q(Period::Week, 0), now, &cat);
        assert_eq!(week.totals.seconds, (30 + 120 + 10 + 30) * 60);
        assert_eq!(week.previous.as_ref().unwrap().seconds, 60 * 60);
        assert_eq!((week.totals.countries, week.totals.languages), (2, 2));
        assert_eq!(week.timeline.len(), 7);
        // stations: CRI, RNZ, unidentified; with the change against the week before
        let st = &week.by.stations;
        assert_eq!(st[0].station.as_ref().unwrap().name, "CRI");
        assert_eq!((st[0].seconds, st[0].delta_s), (150 * 60, 90 * 60));
        assert!(st.iter().any(|s| s.key.is_empty() && s.seconds == 600));
        assert_eq!(week.by.languages.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["M", "E"]);
        assert_eq!(week.by.countries[0].key, "CHN");
        assert_eq!(week.by.bands[0].key, "49m");
        let five = week.distance.iter().find(|b| b.lo_km == 3_000).unwrap();
        assert_eq!(five.sessions, 4);
        // the heat map is UTC: Tuesday 23:00 and Wednesday 00:00
        let total: u32 = week.heat.iter().flatten().sum();
        assert_eq!(total, week.totals.seconds);
        assert_eq!((week.heat[1][23], week.heat[2][0]), (3600, 3600));

        let all = stats(&closed, Some(&open), &q(Period::All, 0), now, &cat);
        assert_eq!(all.from, T0 - 8 * DAY_S);
        assert!(all.previous.is_none());
        assert_eq!(all.totals.sessions, 5);
        assert_eq!(all.clear, ClearCounts { all: 4, older_30: 0, older_90: 0, older_365: 0 });
        // all-time parts ignore the period
        assert_eq!(day.streak, all.streak);
        assert_eq!(day.collection.passport.len(), 2);
    }

    #[test]
    fn local_time_moves_the_day() {
        let cat = catalog();
        // 01:00 UTC on Wednesday is still Tuesday at UTC-4
        let closed = vec![sess(T0 + 3600, 30, None)];
        let now = T0 + 2 * 3600;
        assert_eq!(stats(&closed, None, &q(Period::Day, 0), now, &cat).totals.seconds, 1800);
        let ny = stats(&closed, None, &q(Period::Day, -240), now, &cat);
        assert_eq!((ny.from, ny.totals.seconds), (T0 - 20 * 3600, 1800));
    }

    #[test]
    fn top_ten_and_the_rest() {
        let cat = catalog();
        let closed: Vec<Session> =
            (0..13).map(|i| sess(T0 + i * 600, 5 + i, Some(sref(&format!("S{i:02}"), "CHN", "M")))).collect();
        let s = stats(&closed, None, &q(Period::Week, 0), T0 + DAY_S - 1, &cat);
        assert_eq!(s.by.stations.len(), TOP + 1);
        assert_eq!(s.by.stations[0].station.as_ref().unwrap().name, "S12");
        let other = s.by.stations.last().unwrap();
        assert_eq!((other.key.as_str(), other.sessions, other.seconds), ("*", 3, (5 + 6 + 7) * 60));
    }

    #[test]
    fn streaks_and_milestones() {
        let cat = catalog();
        let day = |d: i64, c: &str| sess(T0 + d * DAY_S + 3600, 5, Some(sref(c, c, "E")));
        let closed = vec![day(-6, "CHN"), day(-5, "CHN"), day(-3, "G"), day(-2, "USA"), day(-1, "CHN")];
        let s = stats(&closed, None, &q(Period::Week, 0), T0 + 600, &cat);
        assert_eq!(s.streak, Streak { current_days: 3, best_days: 3 }, "nothing yet today: still current");
        let s = stats(&closed, None, &q(Period::Week, 0), T0 + DAY_S + 600, &cat);
        assert_eq!(s.streak.current_days, 0);
        let firsts: Vec<_> = s.milestones.iter().filter_map(|m| match &m.what {
            MilestoneKind::FirstCountry { itu } => Some(itu.as_str()),
            _ => None,
        }).collect();
        assert_eq!(firsts, ["USA", "G", "CHN"], "newest first");
        let many: Vec<Session> = (0..10).map(|i| sess(T0 + i * 60, 1, Some(sref(&format!("C{i}"), &format!("C{i}"), "E")))).collect();
        let m = stats(&many, None, &q(Period::All, 0), T0 + 3600, &cat).milestones;
        assert!(m.iter().any(|m| m.what == MilestoneKind::Countries { n: 10 }));
        assert!(m.iter().any(|m| m.what == MilestoneKind::Stations { n: 10 }));
    }

    #[test]
    fn propagation_from_the_sessions() {
        let cat = catalog();
        let sky = |rx| Some(Sky { rx, tx: SunPhase::Day, dark: 0.5, tx_morning: false });
        let at = |h: i64, band: &str, db: i16, rx| Session {
            band: band.into(),
            avg_db: Some(db),
            sky: sky(rx),
            ..sess(T0 + h * 3600, 3, Some(sref("X", "CHN", "M")))
        };
        let mut closed = vec![];
        for d in 0..3 {
            closed.push(Session { start: T0 + d * 60 + 21 * 3600, end: T0 + d * 60 + 21 * 3600 + 180, ..at(21, "49m", -10, SunPhase::Night) });
            closed.push(Session { start: T0 + d * 60 + 21 * 3600 + 30, end: T0 + d * 60 + 21 * 3600 + 210, ..at(21, "31m", -30, SunPhase::Twilight) });
        }
        // two strong 25 m sessions are too few to count
        closed.push(at(21, "25m", 20, SunPhase::Day));
        closed.push(at(21, "25m", 20, SunPhase::Day));
        closed.sort_by_key(|s| s.start);
        let p = stats(&closed, None, &q(Period::Week, 0), T0 + DAY_S - 1, &cat).propagation;
        assert_eq!(p.best_by_hour.len(), 24);
        assert_eq!(p.best_by_hour[21].as_ref().map(|b| (b.band.as_str(), b.db, b.sessions)), Some(("49m", -10, 3)));
        assert!(p.best_by_hour[20].is_none());
        assert_eq!(p.grey_count, 3);
        let b49 = p.day_night.iter().find(|b| b.band == "49m").unwrap();
        assert_eq!((b49.day_s, b49.night_s), (0, 540));
        assert_eq!(p.faint[0].avg_db, Some(-30));
    }

    #[test]
    fn listened_and_trend() {
        let cat = catalog();
        let st = cat.stations().find(|s| s.freq_hz == 6_070_000).unwrap();
        let r = crate::scan::StationRef::of(&cat, st, T0);
        let closed = vec![
            Session { avg_db: Some(-20), ..sess(T0 - 3 * DAY_S, 5, Some(r.clone())) },
            Session { avg_db: Some(-10), ..sess(T0 - 2 * DAY_S, 5, Some(r.clone())) },
            sess(T0, 5, Some(sref("gone", "CHN", "M"))),
        ];
        assert_eq!(listened_ids(&closed, &cat), HashSet::from([st.id]));
        let t = trend_for(&closed, &cat, &st.station_key(), T0 + 60);
        assert_eq!(t.len(), TREND_WEEKS as usize);
        let last = t.last().unwrap();
        assert_eq!((last.db, last.sessions), (Some(-15), 2));
        assert!(t[0].db.is_none());
    }
}
