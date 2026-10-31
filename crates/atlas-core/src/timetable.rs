//! The timetable: the stations a list query keeps, each with its windows on the air today (UTC),
//! for the stage's frequency-by-hour chart. Rows the schedule has none for (signals, favorites
//! off the schedule) have no hours, so they are not in it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::lens::{kept_ids, Known};
use crate::scan::scan_marks;
use crate::schedule::{today_parts, Window};
use crate::stations::{Catalog, StationQuery, StationRow};

/// The most rows one timetable holds; `Timetable::total` says how many there were.
pub const TIMETABLE_MAX_ROWS: usize = 1500;

/// A station in the timetable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimetableRow {
    pub row: StationRow,
    /// Today's (UTC) windows, sorted and merged, with yesterday's past midnight.
    pub windows: Vec<Window>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Timetable {
    /// By frequency, at most `TIMETABLE_MAX_ROWS`.
    pub rows: Vec<TimetableRow>,
    /// Stations matching with some time on the air today, before the cap.
    pub total: u32,
}

/// The stations `q` keeps (its lens and scan filter as in the list, without paging) that are on
/// the air at some time today.
pub fn timetable(cat: &Catalog, q: &StationQuery, unix: i64, k: &Known) -> Timetable {
    capped(cat, q, unix, k, TIMETABLE_MAX_ROWS)
}

fn capped(cat: &Catalog, q: &StationQuery, unix: i64, k: &Known, max: usize) -> Timetable {
    let marks = k.scan.map(|s| scan_marks(cat, s));
    let only = kept_ids(cat, q, k, marks.as_ref());
    let page = cat.matching(&StationQuery { facets: false, ..q.clone() }, unix, only.as_ref());
    let mut rows: Vec<TimetableRow> = page
        .items
        .into_iter()
        .filter_map(|row| {
            let windows = today_parts(&cat.station(row.id)?.slots, unix);
            (!windows.is_empty()).then_some(TimetableRow { row, windows })
        })
        .collect();
    let total = rows.len() as u32;
    rows.truncate(max);
    Timetable { rows, total }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::calendar::{days_from_civil, SECS_PER_DAY};
    use crate::eibi::names::band_of;
    use crate::eibi::{parse, tests::files};
    use crate::favorites::{add, Favorite, FavoriteRequest};
    use crate::rig::Mode;
    use crate::schedule::{Days, SeasonOnly, Slot};
    use crate::source::{Record, SiteSpec, SourceData, SourceDesc, SourceKind};
    use crate::stations::Lens;

    /// `h:m` UTC on 2026-10-`d` (the 8th is a Thursday, the 12th a Monday).
    fn at(d: u32, h: i64, m: i64) -> i64 {
        i64::from(days_from_civil(2026, 10, d)) * SECS_PER_DAY + h * 3600 + m * 60
    }

    fn slot(start: u16, end: u16, days: Days) -> Slot {
        Slot {
            start,
            end,
            days,
            lang: "S".into(),
            target: "SAm".into(),
            season: SeasonOnly::All,
            from: None,
            to: None,
            heard: None,
        }
    }

    /// The fixture's EiBi rows, and two of the listener's at EiBi's site 0: one every night across
    /// midnight, one on Monday mornings only.
    fn catalog() -> Catalog {
        let eibi = parse(&files()).unwrap().into_source();
        let rec = |key: &str, khz: u32, s: Slot| Record {
            key: key.into(),
            freq_hz: khz * 1000,
            mode: Mode::AM,
            name: format!("Radio {key}"),
            itu: "CHL".into(),
            site: SiteSpec::Known(0),
            slots: vec![s],
            notes: String::new(),
        };
        let mine = SourceData {
            desc: SourceDesc {
                id: "mine".into(),
                name: "Mine".into(),
                kind: SourceKind::User,
                editable: true,
                url: None,
                version: "1".into(),
                season: None,
                report: None,
            },
            records: vec![
                rec("night", 6075, slot(1380, 60, Days::Daily)),
                rec("monday", 6080, slot(600, 660, Days::Weekly { mask: 1 })),
            ],
        };
        Catalog::build(&[&eibi.data, &mine], &eibi.vocabulary, &eibi.gazetteer)
    }

    struct World {
        cat: Catalog,
        favorites: Vec<Favorite>,
        listened: HashSet<u32>,
    }

    impl World {
        fn new() -> Self {
            Self { cat: catalog(), favorites: vec![], listened: HashSet::new() }
        }

        fn table(&self, q: &StationQuery, unix: i64, max: usize) -> Timetable {
            let k =
                Known { logbook: &[], signals: &[], favorites: &self.favorites, listened: &self.listened, scan: None };
            capped(&self.cat, q, unix, &k, max)
        }

        fn windows(&self, q: &StationQuery, unix: i64, khz: u32) -> Option<Vec<Window>> {
            let t = self.table(q, unix, TIMETABLE_MAX_ROWS);
            t.rows.into_iter().find(|r| r.row.freq_hz == khz * 1000).map(|r| r.windows)
        }
    }

    fn band(id: &str) -> StationQuery {
        StationQuery { bands: vec![id.into()], ..Default::default() }
    }

    #[test]
    fn a_band_and_its_windows_today() {
        let w = World::new();
        let t = w.table(&band("49m"), at(8, 10, 0), TIMETABLE_MAX_ROWS);
        assert!(!t.rows.is_empty() && t.rows.iter().all(|r| band_of(r.row.freq_hz) == "49m"));
        assert!(t.rows.windows(2).all(|p| p[0].row.freq_hz <= p[1].row.freq_hz));
        assert_eq!(t.total as usize, t.rows.len());
        // New Korea Hope: Tu, Th, Su 17:00-18:00, and Thursday it is
        assert_eq!(w.windows(&band("49m"), at(8, 10, 0), 5900), Some(vec![[1020, 1080]]));
        // the listener's night slot: tonight's start and last night's end
        assert_eq!(w.windows(&band("49m"), at(8, 10, 0), 6075), Some(vec![[0, 60], [1380, 1440]]));
    }

    #[test]
    fn rows_off_the_air_all_day_are_left_out() {
        let w = World::new();
        let q = StationQuery { sources: vec!["mine".into()], ..Default::default() };
        assert_eq!(w.windows(&q, at(8, 10, 0), 6080), None);
        assert_eq!(w.windows(&q, at(12, 10, 0), 6080), Some(vec![[600, 660]]));
        assert_eq!(w.table(&q, at(8, 10, 0), TIMETABLE_MAX_ROWS).total, 1);
    }

    #[test]
    fn lenses_keep_their_stations() {
        let mut w = World::new();
        let all = |lens| StationQuery { lens, ..Default::default() };
        // nothing heard or listened to yet
        assert_eq!(w.table(&all(Lens::Heard), at(8, 10, 0), TIMETABLE_MAX_ROWS).total, 0);
        assert_eq!(w.table(&all(Lens::Listened), at(8, 10, 0), TIMETABLE_MAX_ROWS).total, 0);
        let night = w.cat.by_key("mine", "night").unwrap().clone();
        w.listened.insert(night.id);
        let t = w.table(&all(Lens::Listened), at(8, 10, 0), TIMETABLE_MAX_ROWS);
        assert_eq!(t.rows.iter().map(|r| r.row.id).collect::<Vec<_>>(), [night.id]);
        // a favorite's station, and none for a favorite off the schedule
        let req = |hz, station| FavoriteRequest { freq_hz: hz, mode: Mode::AM, label: Some("x".into()), station };
        add(&mut w.favorites, &req(night.freq_hz, Some(night.station_key())), &w.cat, 1).unwrap();
        add(&mut w.favorites, &req(14_230_000, None), &w.cat, 1).unwrap();
        let t = w.table(&all(Lens::Favorites), at(8, 10, 0), TIMETABLE_MAX_ROWS);
        assert_eq!(t.rows.iter().map(|r| r.row.id).collect::<Vec<_>>(), [night.id]);
    }

    #[test]
    fn the_cap_keeps_the_total() {
        let w = World::new();
        let t = w.table(&StationQuery::default(), at(8, 10, 0), 2);
        assert_eq!(t.rows.len(), 2);
        assert!(t.total > 2);
        assert_eq!(t.rows[0].row.freq_hz, w.table(&StationQuery::default(), at(8, 10, 0), 9).rows[0].row.freq_hz);
    }
}
