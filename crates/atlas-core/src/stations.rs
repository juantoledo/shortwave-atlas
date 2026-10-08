//! Stations, transmitter sites and the "frequency + UTC time -> candidates" lookup.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geo::{dist_bearing, LatLon, Route};
use crate::schedule::{air_status, window_parts, AirStatus, Window};

/// Default match tolerance around the tuned frequency.
pub const LOOKUP_TOLERANCE_HZ: u32 = 1_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub short: String,
    pub lat: f64,
    pub lon: f64,
    /// IANA time zone, e.g. `America/New_York`.
    pub tz: String,
}

impl Site {
    pub fn pos(&self) -> LatLon {
        LatLon { lat: self.lat, lon: self.lon }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub freq_hz: u32,
    pub mode: String,
    pub lang: String,
    pub target: String,
    /// `Site::id` of the transmitter.
    pub site: String,
    pub kw: Option<f64>,
    /// Daily UTC windows (see `schedule`).
    pub sched: Vec<Window>,
    pub note: String,
}

/// A station as seen from a QTH at a given UTC minute.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Candidate {
    pub station: Station,
    pub site: Site,
    pub route: Route,
    pub air: AirStatus,
    /// `station.sched` split at midnight, ready to draw on a 24 h bar.
    pub windows: Vec<Window>,
}

/// Where stations come from: the sample JSON now, SQLite (EiBi) later.
pub trait StationSource: Send + Sync {
    /// Stations within `tol_hz` of `freq_hz`, with their sites.
    fn near(&self, freq_hz: u32, tol_hz: u32) -> Vec<(Station, Site)>;
    /// Every station, with its site.
    fn all(&self) -> Vec<(Station, Site)>;
}

#[derive(Debug)]
pub enum CatalogError {
    Json(serde_json::Error),
    UnknownSite { station: String, site: String },
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid station data: {e}"),
            Self::UnknownSite { station, site } => write!(f, "station {station} refers to unknown site {site}"),
        }
    }
}

impl std::error::Error for CatalogError {}

/// In-memory catalog loaded from JSON (`{"sites": [...], "stations": [...]}`).
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    sites: HashMap<String, Site>,
    stations: Vec<Station>,
}

#[derive(Deserialize)]
struct CatalogFile {
    sites: Vec<Site>,
    stations: Vec<Station>,
}

impl Catalog {
    pub fn from_json(s: &str) -> Result<Self, CatalogError> {
        let file: CatalogFile = serde_json::from_str(s).map_err(CatalogError::Json)?;
        let sites: HashMap<_, _> = file.sites.into_iter().map(|s| (s.id.clone(), s)).collect();
        if let Some(st) = file.stations.iter().find(|st| !sites.contains_key(&st.site)) {
            return Err(CatalogError::UnknownSite { station: st.id.clone(), site: st.site.clone() });
        }
        Ok(Self { sites, stations: file.stations })
    }

    fn with_site(&self, st: &Station) -> (Station, Site) {
        (st.clone(), self.sites[&st.site].clone())
    }
}

impl StationSource for Catalog {
    fn near(&self, freq_hz: u32, tol_hz: u32) -> Vec<(Station, Site)> {
        self.stations.iter().filter(|st| st.freq_hz.abs_diff(freq_hz) <= tol_hz).map(|st| self.with_site(st)).collect()
    }

    fn all(&self) -> Vec<(Station, Site)> {
        self.stations.iter().map(|st| self.with_site(st)).collect()
    }
}

fn candidate(station: Station, site: Site, utc_min: u32, qth: LatLon) -> Candidate {
    Candidate {
        route: dist_bearing(qth, site.pos()),
        air: air_status(&station.sched, utc_min),
        windows: window_parts(&station.sched),
        station,
        site,
    }
}

/// Stations near `freq_hz`: on the air first, then the closest to `qth`.
pub fn lookup(src: &dyn StationSource, freq_hz: u32, utc_min: u32, qth: LatLon, tol_hz: u32) -> Vec<Candidate> {
    let mut out: Vec<_> =
        src.near(freq_hz, tol_hz).into_iter().map(|(st, site)| candidate(st, site, utc_min, qth)).collect();
    out.sort_by(|a, b| b.air.on.cmp(&a.air.on).then(a.route.km.total_cmp(&b.route.km)));
    out
}

/// Every station as a candidate, by frequency then name (the station list and dial marks).
pub fn list(src: &dyn StationSource, utc_min: u32, qth: LatLon) -> Vec<Candidate> {
    let mut out: Vec<_> = src.all().into_iter().map(|(st, site)| candidate(st, site, utc_min, qth)).collect();
    out.sort_by(|a, b| a.station.freq_hz.cmp(&b.station.freq_hz).then_with(|| a.station.name.cmp(&b.station.name)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../data/stations.sample.json");
    const SANTIAGO: LatLon = LatLon { lat: -33.45, lon: -70.67 };

    fn catalog() -> Catalog {
        Catalog::from_json(SAMPLE).expect("sample data loads")
    }

    fn ids(c: &[Candidate]) -> Vec<&str> {
        c.iter().map(|c| c.station.id.as_str()).collect()
    }

    #[test]
    fn sample_has_ten_stations() {
        assert_eq!(catalog().all().len(), 10);
    }

    #[test]
    fn radio_marti_is_in_greenville() {
        let c = lookup(&catalog(), 13_570_000, 600, SANTIAGO, LOOKUP_TOLERANCE_HZ);
        assert_eq!(ids(&c), ["marti-13570"]);
        assert_eq!(c[0].site.id, "greenville");
        assert!(c[0].air.on);
    }

    #[test]
    fn tolerance_is_one_khz_each_way() {
        let cat = catalog();
        assert_eq!(lookup(&cat, 13_571_000, 600, SANTIAGO, LOOKUP_TOLERANCE_HZ).len(), 1);
        assert_eq!(lookup(&cat, 13_569_000, 600, SANTIAGO, LOOKUP_TOLERANCE_HZ).len(), 1);
        assert!(lookup(&cat, 13_571_100, 600, SANTIAGO, LOOKUP_TOLERANCE_HZ).is_empty());
    }

    #[test]
    fn on_air_first_then_closest() {
        // 15 MHz: WWV (Colorado), WWVH (Hawaii), BPM (China, off 01:00-07:00 UTC)
        let c = lookup(&catalog(), 15_000_000, 120, SANTIAGO, LOOKUP_TOLERANCE_HZ);
        assert_eq!(ids(&c), ["wwv-15000", "wwvh-15000", "bpm-15000"]);
        assert!(!c[2].air.on);
    }

    #[test]
    fn list_is_sorted_by_frequency() {
        let l = list(&catalog(), 0, SANTIAGO);
        assert!(l.windows(2).all(|w| w[0].station.freq_hz <= w[1].station.freq_hz));
        assert_eq!(l.first().unwrap().station.id, "rebelde-5025");
    }

    #[test]
    fn unknown_site_is_rejected() {
        let bad = r#"{"sites": [], "stations": [{"id":"x","name":"X","freq_hz":1,"mode":"AM","lang":"","target":"","site":"nowhere","kw":null,"sched":[],"note":""}]}"#;
        assert!(matches!(Catalog::from_json(bad), Err(CatalogError::UnknownSite { .. })));
    }
}
