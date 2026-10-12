//! Nudges from the history: what to hunt now (on the air, never heard, and a path that
//! suits the band), what the band scans logged but the listener never stayed on, and the
//! station they usually listen to at this hour.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::collect::rarity;
use crate::eibi::names::{band_of, OUT_OF_BAND};
use crate::geo::{dark_share, dist_bearing, LatLon};
use crate::listening::Session;
use crate::rig::{Mode, RxRange};
use crate::scan::{Logged, StationRef};
use crate::schedule::any_on;
use crate::stations::Catalog;
use crate::stats::NowListening;

/// Rows in "hunt now" and "scanned but never listened".
pub const HUNT_ROWS: usize = 10;
pub const SCANNED_ROWS: usize = 20;
/// "Your usual": the last 30 days, an hour either side of now.
const USUAL_DAYS: i64 = 30;

/// Why a station is worth a try.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum HuntReason {
    /// A low frequency over a path in the dark.
    DarkPath,
    /// A high frequency over a path in daylight.
    LitPath,
    NewCountry,
    NewStation,
    Rare { stars: u8 },
    Far { km: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HuntRow {
    pub station_id: u32,
    pub station: StationRef,
    pub mode: Mode,
    pub km: u32,
    pub rarity: u8,
    pub reasons: Vec<HuntReason>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScannedRow {
    pub station_id: u32,
    pub station: StationRef,
    pub mode: Mode,
    pub best_db: i16,
    #[ts(type = "number")]
    pub last: i64,
}

/// Result of `Call::Hunt`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Hunt {
    pub now: Vec<HuntRow>,
    pub scanned: Vec<ScannedRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Usual {
    pub station_id: u32,
    pub station: StationRef,
    pub mode: Mode,
    /// Listened at this hour over the last 30 days.
    pub seconds: u32,
}

/// Result of `Call::Listening`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListeningNow {
    pub now: Option<NowListening>,
    /// When nothing is being recorded: the usual station at this hour, if it is on.
    pub usual: Option<Usual>,
}

/// How well the path suits the frequency now (0..=1): the lower bands travel in the dark, the
/// higher ones in daylight.
pub fn path_fit(freq_hz: u32, dark: f32) -> f32 {
    match freq_hz {
        f if f < 10_000_000 => dark,
        f if f > 13_000_000 => 1.0 - dark,
        _ => 0.5,
    }
}

/// Stations on the air now, in the rig's coverage, that the history has not got; the best one
/// per country, countries never heard first.
pub fn hunt_now(cat: &Catalog, all: &[&Session], now: i64, qth: LatLon, coverage: &[RxRange]) -> Vec<HuntRow> {
    let countries: HashSet<&str> = all.iter().filter_map(|s| s.country()).collect();
    let heard: HashSet<String> = all.iter().filter_map(|s| s.station.as_ref().map(StationRef::ident)).collect();
    let mut best: HashMap<String, (f32, HuntRow)> = HashMap::new();
    for st in cat.stations() {
        if band_of(st.freq_hz) == OUT_OF_BAND
            || !coverage.iter().any(|r| r.contains_hz(st.freq_hz))
            || !any_on(&st.slots, now)
        {
            continue;
        }
        let r = StationRef::of(cat, st, now);
        if heard.contains(&r.ident()) {
            continue;
        }
        let Some(site) = cat.site(st.site) else { continue };
        let km = dist_bearing(qth, site.pos()).km;
        let fit = path_fit(st.freq_hz, dark_share(qth, site.pos(), now));
        let stars = rarity(&st.slots);
        let new_country = !st.itu.is_empty() && !countries.contains(st.itu.as_str());
        let score = fit * 2.0
            + f32::from(stars - 1) * 0.5
            + (km / 20_000.0).min(1.0) as f32
            + if new_country { 3.0 } else { 0.0 };
        let mut reasons = vec![];
        if fit >= 0.75 {
            reasons.push(if st.freq_hz < 10_000_000 { HuntReason::DarkPath } else { HuntReason::LitPath });
        }
        reasons.push(if new_country { HuntReason::NewCountry } else { HuntReason::NewStation });
        if stars >= 4 {
            reasons.push(HuntReason::Rare { stars });
        }
        if km >= 10_000.0 {
            reasons.push(HuntReason::Far { km: km as u32 });
        }
        let row = HuntRow { station_id: st.id, station: r, mode: st.mode, km: km as u32, rarity: stars, reasons };
        let e = best.entry(st.itu.clone()).or_insert((f32::MIN, row.clone()));
        if score > e.0 {
            *e = (score, row);
        }
    }
    let mut rows: Vec<(f32, HuntRow)> = best.into_values().collect();
    rows.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.station.freq_hz.cmp(&b.1.station.freq_hz)));
    rows.into_iter().take(HUNT_ROWS).map(|(_, r)| r).collect()
}

/// Stations the band scans logged that the listener never stayed on, last logged first.
pub fn scanned_not_listened(cat: &Catalog, all: &[&Session], logbook: &[Logged], now: i64) -> Vec<ScannedRow> {
    let heard: HashSet<String> = all.iter().filter_map(|s| s.station.as_ref().map(StationRef::ident)).collect();
    let mut rows: Vec<ScannedRow> = logbook
        .iter()
        .filter_map(|l| {
            let st = cat.station(l.station_id)?;
            let r = StationRef::of(cat, st, now);
            (!heard.contains(&r.ident())).then_some(ScannedRow {
                station_id: st.id,
                station: r,
                mode: st.mode,
                best_db: l.best_db,
                last: l.last,
            })
        })
        .collect();
    rows.sort_by(|a, b| b.last.cmp(&a.last).then(a.station_id.cmp(&b.station_id)));
    rows.truncate(SCANNED_ROWS);
    rows
}

/// The station listened to most at this UTC hour (±1) over the last 30 days, if it is on the
/// air now.
pub fn usual(cat: &Catalog, all: &[&Session], now: i64) -> Option<Usual> {
    let hour = |t: i64| t.rem_euclid(86_400) / 3600;
    let h = hour(now);
    let mut by: HashMap<String, (i64, &StationRef)> = HashMap::new();
    for s in all.iter().filter(|s| s.end > now - USUAL_DAYS * 86_400) {
        let Some(r) = &s.station else { continue };
        let d = (hour(s.start) - h).rem_euclid(24);
        if d <= 1 || d == 23 {
            by.entry(r.ident()).or_insert((0, r)).0 += s.secs();
        }
    }
    let mut ranked: Vec<(i64, &StationRef)> = by.into_values().collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    ranked.into_iter().find_map(|(secs, r)| {
        let st = r.find(cat).filter(|st| any_on(&st.slots, now))?;
        Some(Usual {
            station_id: st.id,
            station: StationRef::of(cat, st, now),
            mode: st.mode,
            seconds: secs.clamp(0, i64::from(u32::MAX)) as u32,
        })
    })
}

/// Everything `Call::Hunt` returns.
pub fn hunt(cat: &Catalog, all: &[&Session], logbook: &[Logged], now: i64, qth: LatLon, coverage: &[RxRange]) -> Hunt {
    Hunt { now: hunt_now(cat, all, now, qth, coverage), scanned: scanned_not_listened(cat, all, logbook, now) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::tests::sess;
    use crate::listening::tests::{catalog, QTH, T0};
    use crate::rig::default_coverage;

    fn on_air_ref(cat: &Catalog, freq_hz: u32, at: i64) -> (u32, StationRef) {
        let st = cat.stations().find(|s| s.freq_hz == freq_hz).unwrap();
        (st.id, StationRef::of(cat, st, at))
    }

    #[test]
    fn path_fit_by_band() {
        assert_eq!(path_fit(6_000_000, 1.0), 1.0);
        assert_eq!(path_fit(6_000_000, 0.0), 0.0);
        assert_eq!(path_fit(17_000_000, 0.0), 1.0);
        assert_eq!(path_fit(11_000_000, 1.0), 0.5);
    }

    #[test]
    fn hunting_skips_what_was_heard_and_what_the_rig_cannot_receive() {
        let cat = catalog();
        // 00:30 UTC: CRI on 6070 and 11000/9000-ish stations, KBS on 11000, Radio Nowhere...
        let at = T0 + 1800;
        let rows = hunt_now(&cat, &[], at, QTH, &default_coverage());
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| r.reasons.contains(&HuntReason::NewCountry)), "{rows:?}");
        let countries: Vec<&str> = rows.iter().map(|r| r.station.itu.as_str()).collect();
        let unique: HashSet<&&str> = countries.iter().collect();
        assert_eq!(unique.len(), countries.len(), "one per country");
        // CHN heard: China drops out; out of coverage: nothing
        let (_, cri) = on_air_ref(&cat, 6_070_000, at);
        let heard = vec![sess(T0 - 3600, 10, Some(cri))];
        let rows2 = hunt_now(&cat, &heard.iter().collect::<Vec<_>>(), at, QTH, &default_coverage());
        assert!(rows2.iter().all(|r| r.station.itu != "CHN" || r.reasons.contains(&HuntReason::NewStation)));
        assert!(rows2.iter().all(|r| r.station.freq_hz != 6_070_000));
        let none = hunt_now(&cat, &[], at, QTH, &[RxRange { lo_khz: 100, hi_khz: 200 }]);
        assert!(none.is_empty());
    }

    #[test]
    fn scanned_and_usual() {
        let cat = catalog();
        let at = T0 + 1800;
        let (cri_id, cri) = on_air_ref(&cat, 6_070_000, at);
        let (kbs_id, _) = on_air_ref(&cat, 11_000_000, at);
        let log = |id, last| Logged { station_id: id, site_id: 0, first: last, last, best_db: -10, count: 1, sure: true };
        let heard = vec![sess(T0 - 86_400 + 600, 40, Some(cri.clone()))];
        let heard: Vec<&Session> = heard.iter().collect();
        let rows = scanned_not_listened(&cat, &heard, &[log(cri_id, 5), log(kbs_id, 9)], at);
        assert_eq!(rows.iter().map(|r| r.station_id).collect::<Vec<_>>(), [kbs_id]);
        // listened yesterday at 00:10: the usual at 00:30, and not at 06:00
        let u = usual(&cat, &heard, at).unwrap();
        assert_eq!((u.station_id, u.seconds), (cri_id, 2400));
        assert!(usual(&cat, &heard, T0 + 6 * 3600).is_none());
        // too long ago
        let old = vec![sess(T0 - 40 * 86_400, 40, Some(cri))];
        assert!(usual(&cat, &[&old[0]], at).is_none());
    }
}
