//! Data sources: where stations come from. EiBi's schedule is one; others (the listener's own
//! stations, other schedules) plug in the same way. A source turns its data into `Record`s, and
//! the catalog is built from every source's records against the code names (`Vocabulary`) and
//! transmitter sites (`Gazetteer`) they share.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::calendar::{day_of, iso_date, Season};
use crate::eibi::readme::Lang;
use crate::eibi::{ParseReport, SiteRec};
use crate::rig::Mode;
use crate::schedule::Slot;

/// `SourceDesc::id` of the EiBi schedule.
pub const EIBI: &str = "eibi";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SourceKind {
    /// A published schedule.
    Schedule,
    /// Stations the listener keeps.
    User,
}

/// A source as the catalog keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceDesc {
    /// `eibi`
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    /// Whether the listener may change its stations.
    pub editable: bool,
    pub url: Option<String>,
    /// Which version of the data (`sked-a26.csv`).
    pub version: String,
    pub season: Option<Season>,
    pub report: Option<ParseReport>,
}

/// What the UI shows about a source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub editable: bool,
    pub url: Option<String>,
    pub version: String,
    /// `A26`
    pub season: Option<String>,
    /// Season dates, `YYYY-MM-DD`.
    pub from: Option<String>,
    pub to: Option<String>,
    /// Whether the data holds now: a schedule only in its season (and not when its season is
    /// unknown); the listener's own stations always.
    pub current: bool,
    /// Catalog rows from this source.
    pub rows: u32,
    pub report: Option<ParseReport>,
}

/// Which version of a source something was matched against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SourceStamp {
    pub id: String,
    pub version: String,
}

impl SourceDesc {
    pub fn info(&self, rows: u32, unix: i64) -> SourceInfo {
        let range = self.season.map(|s| s.range());
        SourceInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind,
            editable: self.editable,
            url: self.url.clone(),
            version: self.version.clone(),
            season: self.season.map(|s| s.code()),
            from: range.map(|r| iso_date(day_of(r.0))),
            to: range.map(|r| iso_date(day_of(r.1) - 1)),
            current: match self.kind {
                SourceKind::Schedule => self.season.is_some_and(|s| s == Season::at(unix)),
                SourceKind::User => true,
            },
            rows,
            report: self.report.clone(),
        }
    }

    pub fn stamp(&self) -> SourceStamp {
        SourceStamp { id: self.id.clone(), version: self.version.clone() }
    }
}

/// Where a record's transmitter stands.
#[derive(Clone, Debug, PartialEq)]
pub enum SiteSpec {
    /// `Gazetteer::sites[i]`.
    Known(u32),
    /// A site of the source's own.
    At(SiteRec),
}

/// One station on one frequency from one site, with its schedule.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// Identifies the record within its source, across versions of the data.
    pub key: String,
    pub freq_hz: u32,
    pub mode: Mode,
    pub name: String,
    /// The station's own country (EiBi code).
    pub itu: String,
    pub site: SiteSpec,
    pub slots: Vec<Slot>,
    /// Free text about it, which search finds.
    pub notes: String,
}

/// A source's records.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceData {
    pub desc: SourceDesc,
    pub records: Vec<Record>,
}

/// Language and target names beyond the built-in tables (from EiBi's README). Every source uses
/// these codes; one with codes of its own maps them when it builds its records.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vocabulary {
    pub langs: BTreeMap<String, Lang>,
    pub targets: BTreeMap<String, String>,
}

/// Known transmitter sites, and where a country's sites stand when a record does not say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gazetteer {
    pub sites: Vec<SiteRec>,
    /// A known site of each country (EiBi's fallback for unknown sites).
    pub first_site: HashMap<String, (f64, f64)>,
}

/// A schedule row's key: frequency, station, its country and the site's.
pub fn row_key(freq_hz: u32, name: &str, itu: &str, site_itu: &str, site_name: &str) -> String {
    format!("{freq_hz}|{name}|{itu}|{site_itu}|{site_name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::{days_from_civil, SECS_PER_DAY};

    fn desc(season: Option<Season>) -> SourceDesc {
        SourceDesc {
            id: EIBI.into(),
            name: "EiBi".into(),
            kind: SourceKind::Schedule,
            editable: false,
            url: None,
            version: "sked-a26.csv".into(),
            season,
            report: None,
        }
    }

    #[test]
    fn info_says_whether_the_season_holds() {
        let a26 = Season::from_filename("sked-a26.csv");
        let oct = i64::from(days_from_civil(2026, 10, 8)) * SECS_PER_DAY;
        let i = desc(a26).info(7, oct);
        assert_eq!(
            (i.season.as_deref(), i.from.as_deref(), i.to.as_deref()),
            (Some("A26"), Some("2026-03-29"), Some("2026-10-24"))
        );
        assert!(i.current && i.rows == 7);
        assert!(!desc(a26).info(0, oct + 30 * SECS_PER_DAY).current);
        // a schedule of unknown season is not taken as current; the listener's stations are
        assert!(!desc(None).info(0, 0).current);
        assert!(SourceDesc { kind: SourceKind::User, ..desc(None) }.info(0, 0).current);
        assert_eq!(desc(None).stamp(), SourceStamp { id: "eibi".into(), version: "sked-a26.csv".into() });
    }

    #[test]
    fn keys() {
        assert_eq!(row_key(6_070_000, "CNR", "CHN", "CHN", "China"), "6070000|CNR|CHN|CHN|China");
    }
}
