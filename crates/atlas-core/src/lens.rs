//! The station list's lenses: the whole schedule, what the band scans heard, or the favorites,
//! optionally narrowed to one saved scan. A lens keeps catalog rows by station id, and adds the
//! rows the schedule has none for: signals no station explained and favorites off the schedule.
//! Heard is one list of both, across bands, in the order asked for. Favorites is every favorite,
//! one row each, whatever the filters but the text.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::eibi::names::band_of;
use crate::favorites::{off_schedule, Favorite};
use crate::rig::Mode;
use crate::scan::{scan_marks, unidentified, Logged, ScanMarks, ScanSummary, Signal, StationRef};
use crate::stations::{page_limit, Catalog, Lens, Sort, StationPage, StationQuery, StationRow, LOOKUP_TOLERANCE_HZ};
use crate::text::fold;

/// A row of the station list the schedule has no station for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OffSchedule {
    pub freq_hz: u32,
    /// The favorite's mode (a signal has none).
    pub mode: Option<Mode>,
    /// The favorite's label, empty for a signal.
    pub label: String,
    /// `Favorite::id`, when one is kept there.
    pub favorite: Option<String>,
    /// The station saved with the favorite, which the schedule no longer has.
    pub station: Option<StationRef>,
    /// The scans heard an unidentified signal there.
    pub heard: Option<Signal>,
}

/// A row of the Heard lens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[ts(export)]
pub enum HeardRow {
    /// A station the scans logged, with what they read.
    Station { row: StationRow, log: Logged },
    /// A signal no station explained (`row.heard` is set).
    Signal { row: OffSchedule },
}

impl HeardRow {
    fn freq_hz(&self) -> u32 {
        match self {
            Self::Station { row, .. } => row.freq_hz,
            Self::Signal { row } => row.freq_hz,
        }
    }

    /// The best reading and when it was last heard.
    fn heard(&self) -> (i16, i64) {
        match self {
            Self::Station { log, .. } => (log.best_db, log.last),
            Self::Signal { row } => row.heard.as_ref().map_or((i16::MIN, 0), |s| (s.best_db, s.last)),
        }
    }
}

/// A row of the Favorites lens: one per favorite, so two of one station are two rows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[ts(export)]
pub enum FavoriteRow {
    /// A favorite whose station the schedule has; `row` is that station now.
    Station { row: Box<StationRow>, favorite: Favorite },
    /// Saved without a station, or with one the schedule no longer has.
    Off { row: OffSchedule },
}

impl FavoriteRow {
    fn freq_hz(&self) -> u32 {
        match self {
            Self::Station { favorite, .. } => favorite.freq_hz,
            Self::Off { row } => row.freq_hz,
        }
    }
}

/// What the lenses draw on besides the catalog.
pub struct Known<'a> {
    /// The stations the scans logged.
    pub logbook: &'a [Logged],
    /// Unidentified signals across every scan.
    pub signals: &'a [Signal],
    pub favorites: &'a [Favorite],
    /// Station ids the listening history has (`stats::listened_ids`).
    pub listened: &'a HashSet<u32>,
    /// The scan `StationQuery::scan` names, if it is still saved.
    pub scan: Option<&'a ScanSummary>,
}

fn near(a: u32, b: u32) -> bool {
    a.abs_diff(b) <= LOOKUP_TOLERANCE_HZ
}

/// The station list for `q`: catalog rows the lens and scan filter keep, and the rows off the
/// schedule that pass the filters; for Heard (without a scan filter), both in one list.
pub fn search(cat: &Catalog, q: &StationQuery, unix: i64, k: &Known) -> StationPage {
    if q.lens == Lens::Favorites {
        return favorites(cat, q, unix, k);
    }
    let off_favs = off_schedule(k.favorites, cat);
    let marks = k.scan.map(|s| scan_marks(cat, s));
    let only = kept_ids(cat, q, k, marks.as_ref());
    let heard_list = q.lens == Lens::Heard && q.scan.is_none();
    let mut page =
        if heard_list { cat.matching(q, unix, only.as_ref()) } else { cat.search_in(q, unix, only.as_ref()) };
    if let Some(m) = &marks {
        page.scan_db = page.items.iter().filter_map(|r| m.heard.get(&r.id).map(|&db| (r.id, db))).collect();
    }

    // a scan filter whose scan is gone keeps nothing
    if q.scan.is_some() && k.scan.is_none() {
        return page;
    }
    let fav_at = |hz: u32| off_favs.iter().find(|f| near(f.freq_hz, hz));
    let from_signal = |s: &Signal| {
        let f = fav_at(s.freq_hz);
        OffSchedule {
            freq_hz: s.freq_hz,
            mode: f.map(|f| f.mode),
            label: f.map(|f| f.label.clone()).unwrap_or_default(),
            favorite: f.map(|f| f.id.clone()),
            station: f.and_then(|f| f.station.clone()),
            heard: Some(s.clone()),
        }
    };
    if heard_list {
        let logs: HashMap<u32, &Logged> = k.logbook.iter().map(|l| (l.station_id, l)).collect();
        let stations = std::mem::take(&mut page.items).into_iter();
        let mut rows: Vec<HeardRow> =
            stations.filter_map(|row| logs.get(&row.id).map(|&l| HeardRow::Station { log: l.clone(), row })).collect();
        let signals = filter_off(k.signals.iter().map(from_signal).collect(), q);
        rows.extend(signals.into_iter().map(|row| HeardRow::Signal { row }));
        // stable: the same frequency keeps stations first, in catalog order
        match q.sort {
            Sort::Freq => rows.sort_by_key(HeardRow::freq_hz),
            Sort::Strongest => rows.sort_by_key(|r| (Reverse(r.heard().0), r.freq_hz())),
            Sort::Recent => rows.sort_by_key(|r| (Reverse(r.heard().1), r.freq_hz())),
        }
        page.total = rows.len() as u32;
        page.heard = rows.into_iter().skip(q.offset as usize).take(page_limit(q) as usize).collect();
        return page;
    }
    let rows: Vec<OffSchedule> = match k.scan {
        Some(s) => unidentified([s]).iter().map(from_signal).collect(),
        None => vec![],
    };
    page.off_schedule = filter_off(rows, q);
    page
}

/// The station ids the lens and the scan filter keep (`marks` is that scan's), `None` for every
/// one. Under a scan filter whose scan is gone `Catalog::matching` keeps nothing anyway.
pub(crate) fn kept_ids(cat: &Catalog, q: &StationQuery, k: &Known, marks: Option<&ScanMarks>) -> Option<HashSet<u32>> {
    let lens: Option<HashSet<u32>> = match q.lens {
        Lens::All => None,
        Lens::Heard => Some(k.logbook.iter().map(|l| l.station_id).collect()),
        Lens::Listened => Some(k.listened.clone()),
        Lens::Favorites => Some(favorite_ids(cat, k.favorites).into_iter().flatten().collect()),
    };
    match (lens, marks) {
        (lens, None) => lens,
        (None, Some(m)) => Some(m.ids()),
        (Some(l), Some(m)) => Some(m.ids().intersection(&l).copied().collect()),
    }
}

/// Each favorite's station id in `cat`, if it has one there.
fn favorite_ids(cat: &Catalog, favorites: &[Favorite]) -> Vec<Option<u32>> {
    favorites.iter().map(|f| f.station.as_ref().and_then(|s| s.find(cat)).map(|st| st.id)).collect()
}

/// The rows off the schedule that the band and text filters keep. The others (language,
/// country, region, site, source) are about catalog rows: with any of them set, none.
fn filter_off(mut rows: Vec<OffSchedule>, q: &StationQuery) -> Vec<OffSchedule> {
    if q.site.is_some()
        || !q.langs.is_empty()
        || !q.countries.is_empty()
        || !q.regions.is_empty()
        || !q.sources.is_empty()
    {
        return vec![];
    }
    let text = text_filter(&q.q);
    rows.retain(|r| {
        let band_ok = q.bands.is_empty() || q.bands.iter().any(|b| b == band_of(r.freq_hz));
        band_ok && text(r.freq_hz, &saved_text(&r.label, r.station.as_ref()))
    });
    rows.sort_by_key(|r| r.freq_hz);
    rows
}

/// The Favorites lens: every favorite, one row each, by frequency. Only the text narrows it; the
/// other filters (band, language, country, region, source, site, scan) are about the schedule.
fn favorites(cat: &Catalog, q: &StationQuery, unix: i64, k: &Known) -> StationPage {
    let found = favorite_ids(cat, k.favorites);
    let ids: HashSet<u32> = found.iter().flatten().copied().collect();
    let all = StationQuery { lens: Lens::Favorites, ..Default::default() };
    let mut page = cat.matching(&StationQuery { facets: q.facets, ..all.clone() }, unix, Some(&ids));
    let stations: HashMap<u32, StationRow> = std::mem::take(&mut page.items).into_iter().map(|r| (r.id, r)).collect();
    // the stations the query names by what the schedule says of them (country, language, target)
    let named: HashSet<u32> = if q.q.trim().is_empty() {
        ids
    } else {
        cat.matching(&StationQuery { q: q.q.clone(), ..all }, unix, Some(&ids)).items.iter().map(|r| r.id).collect()
    };
    let text = text_filter(&q.q);
    let mut rows: Vec<(FavoriteRow, i64)> = k
        .favorites
        .iter()
        .zip(found)
        .filter(|(f, id)| {
            text(f.freq_hz, &saved_text(&f.label, f.station.as_ref())) || id.is_some_and(|id| named.contains(&id))
        })
        .map(|(f, id)| {
            let row = match id.and_then(|id| stations.get(&id)) {
                Some(row) => FavoriteRow::Station { row: Box::new(row.clone()), favorite: f.clone() },
                None => FavoriteRow::Off { row: off_favorite(f, k.signals) },
            };
            (row, f.added)
        })
        .collect();
    rows.sort_by_key(|(r, added)| (r.freq_hz(), *added));
    page.total = rows.len() as u32;
    page.on_air = rows.iter().filter(|(r, _)| matches!(r, FavoriteRow::Station { row, .. } if row.on)).count() as u32;
    page.favorites = rows.into_iter().map(|(r, _)| r).skip(q.offset as usize).take(page_limit(q) as usize).collect();
    page
}

/// A favorite the schedule has no row for, with the unidentified signal heard there, if any.
fn off_favorite(f: &Favorite, signals: &[Signal]) -> OffSchedule {
    OffSchedule {
        freq_hz: f.freq_hz,
        mode: Some(f.mode),
        label: f.label.clone(),
        favorite: Some(f.id.clone()),
        station: f.station.clone(),
        heard: signals.iter().find(|s| near(s.freq_hz, f.freq_hz)).cloned(),
    }
}

/// What the text filter reads of a row without a catalog row: its label and saved station name.
fn saved_text(label: &str, station: Option<&StationRef>) -> String {
    format!("{label} {}", station.map_or("", |s| s.name.as_str()))
}

/// The text filter on rows without a catalog row: digits are a kHz prefix of the frequency,
/// otherwise every word must be in the row's text.
fn text_filter(q: &str) -> impl Fn(u32, &str) -> bool {
    let text = fold(q.trim());
    let khz_prefix = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    move |freq_hz, hay| {
        if khz_prefix {
            format!("{}", freq_hz as f64 / 1000.0).starts_with(&text)
        } else {
            let hay = fold(hay);
            text.split_whitespace().all(|t| hay.contains(t))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Qth;
    use crate::bands::BandKind;
    use crate::eibi::{parse, tests::files};
    use crate::favorites::{add, FavoriteRequest};
    use crate::scan::{Peak, ScanOutcome, Verdict, FORMAT_VERSION};

    fn catalog() -> Catalog {
        Catalog::from_eibi(parse(&files()).unwrap())
    }

    fn scan(id: &str, peaks: Vec<Peak>, missed: Vec<StationRef>) -> ScanSummary {
        ScanSummary {
            version: FORMAT_VERSION,
            id: id.into(),
            band_id: "49m".into(),
            band_label: "49".into(),
            band_kind: BandKind::Broadcast,
            first_hz: 5_800_000,
            step_hz: 5_000,
            points: 101,
            dwell_ms: 300,
            reads: 3,
            mode: None,
            started: 0,
            finished: 1,
            outcome: ScanOutcome::Complete,
            qth: Qth::default(),
            rig: "sim".into(),
            sources: vec![],
            noise_db: Some(-48),
            stuck_db: None,
            measured: 101,
            peaks,
            missed,
        }
    }

    fn unknown(khz: u32, db: i16, at: i64) -> Peak {
        Peak { freq_hz: khz * 1000, db, at, verdict: Verdict::Unidentified, stations: vec![] }
    }

    struct World {
        cat: Catalog,
        logbook: Vec<Logged>,
        favorites: Vec<Favorite>,
        scans: Vec<ScanSummary>,
        listened: HashSet<u32>,
    }

    impl World {
        fn new() -> Self {
            let cat = catalog();
            let (a, b, c) = (cat.station(0).unwrap(), cat.station(3).unwrap(), cat.station(5).unwrap());
            let r = |id: u32| crate::scan::StationRef::of(&cat, cat.station(id).unwrap(), 0);
            let heard = Peak { freq_hz: a.freq_hz, db: -12, at: 30, verdict: Verdict::Sure, stations: vec![r(a.id)] };
            let scans = vec![
                scan("s1", vec![heard, unknown(5905, -20, 5)], vec![r(b.id)]),
                scan("s2", vec![unknown(6200, -10, 20)], vec![]),
            ];
            let mut favorites = vec![];
            let req = |freq_hz, station_id: Option<u32>, label: Option<&str>| FavoriteRequest {
                freq_hz,
                mode: Mode::AM,
                label: label.map(Into::into),
                station: station_id.map(|id| cat.station(id).unwrap().station_key()),
            };
            add(&mut favorites, &req(c.freq_hz, Some(c.id), None), &cat, 1).unwrap();
            add(&mut favorites, &req(a.freq_hz, Some(a.id), None), &cat, 1).unwrap();
            add(&mut favorites, &req(6_200_000, None, Some("Mystery carrier")), &cat, 1).unwrap();
            add(&mut favorites, &req(14_230_000, None, Some("SSTV")), &cat, 1).unwrap();
            let logbook = crate::scan::logbook(&scans, &cat);
            let listened = HashSet::from([b.id, c.id]);
            Self { cat, logbook, favorites, scans, listened }
        }

        fn search(&self, q: &StationQuery) -> StationPage {
            let signals = unidentified(&self.scans);
            let scan = q.scan.as_ref().and_then(|id| self.scans.iter().find(|s| &s.id == id));
            search(
                &self.cat,
                q,
                0,
                &Known {
                    logbook: &self.logbook,
                    signals: &signals,
                    favorites: &self.favorites,
                    listened: &self.listened,
                    scan,
                },
            )
        }
    }

    fn ids(p: &StationPage) -> Vec<u32> {
        let mut v: Vec<u32> = p.items.iter().map(|r| r.id).collect();
        v.sort_unstable();
        v
    }

    fn off(p: &StationPage) -> Vec<(u32, &str, bool, bool)> {
        p.off_schedule
            .iter()
            .map(|r| (r.freq_hz / 1000, r.label.as_str(), r.favorite.is_some(), r.heard.is_some()))
            .collect()
    }

    #[test]
    fn the_listened_lens_keeps_the_history() {
        let w = World::new();
        let (b, c) = (w.cat.station(3).unwrap().id, w.cat.station(5).unwrap().id);
        let p = w.search(&StationQuery { lens: Lens::Listened, ..Default::default() });
        assert_eq!((ids(&p), p.off_schedule.len()), (vec![b, c], 0));
        // narrowed to a scan, only what it covered
        let p = w.search(&StationQuery { lens: Lens::Listened, scan: Some("s2".into()), ..Default::default() });
        assert!(p.items.is_empty());
    }

    #[test]
    fn the_whole_schedule_has_no_extra_rows() {
        let w = World::new();
        let p = w.search(&StationQuery::default());
        assert_eq!(p.total as usize, w.cat.len());
        assert!(p.off_schedule.is_empty() && p.scan_db.is_empty());
    }

    /// The Heard list as (kHz, a station?).
    fn heard(p: &StationPage) -> Vec<(u32, bool)> {
        p.heard.iter().map(|r| (r.freq_hz() / 1000, matches!(r, HeardRow::Station { .. }))).collect()
    }

    #[test]
    fn heard_is_one_list_of_the_logbook_and_the_unknown_signals() {
        let w = World::new();
        let a = w.cat.station(0).unwrap().freq_hz / 1000;
        let q = |sort, f: StationQuery| w.search(&StationQuery { lens: Lens::Heard, sort, ..f });
        let p = q(Sort::Strongest, StationQuery::default());
        assert!(p.items.is_empty() && p.off_schedule.is_empty());
        assert_eq!(p.total, 3);
        assert_eq!(heard(&p), [(6200, false), (a, true), (5905, false)]);
        match &p.heard[..] {
            [HeardRow::Signal { row: sig }, HeardRow::Station { row, log }, _] => {
                assert_eq!((sig.label.as_str(), sig.heard.as_ref().map(|h| h.best_db)), ("Mystery carrier", Some(-10)));
                assert_eq!((row.id, log.best_db, log.count, log.last), (0, -12, 1, 30));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(heard(&q(Sort::Recent, StationQuery::default())), [(a, true), (6200, false), (5905, false)]);
        let mut by_freq = vec![(a, true), (5905, false), (6200, false)];
        by_freq.sort_by_key(|r| r.0);
        assert_eq!(heard(&q(Sort::Freq, StationQuery::default())), by_freq);

        // pages go through both kinds
        let p = q(Sort::Strongest, StationQuery { offset: 1, limit: 1, ..Default::default() });
        assert_eq!((p.total, heard(&p)), (3, vec![(a, true)]));
        // the text filter narrows both; a schedule-only filter drops the signals
        assert_eq!(heard(&q(Sort::Freq, StationQuery { q: "mystery".into(), ..Default::default() })), [(6200, false)]);
        let langs = q(Sort::Freq, StationQuery { langs: vec!["E".into()], ..Default::default() });
        assert!(langs.heard.iter().all(|r| matches!(r, HeardRow::Station { .. })));
    }

    /// The Favorites list as (kHz, a station row?, label).
    fn favs(p: &StationPage) -> Vec<(u32, bool, &str)> {
        p.favorites
            .iter()
            .map(|r| match r {
                FavoriteRow::Station { favorite, .. } => (favorite.freq_hz / 1000, true, favorite.label.as_str()),
                FavoriteRow::Off { row } => (row.freq_hz / 1000, false, row.label.as_str()),
            })
            .collect()
    }

    #[test]
    fn favorites_are_one_list_by_frequency() {
        let w = World::new();
        let (a, c) = (w.cat.station(0).unwrap(), w.cat.station(5).unwrap());
        let p = w.search(&StationQuery { lens: Lens::Favorites, ..Default::default() });
        assert!(p.items.is_empty() && p.off_schedule.is_empty());
        let mut want = vec![
            (a.freq_hz / 1000, true, a.name.as_str()),
            (c.freq_hz / 1000, true, c.name.as_str()),
            (6200, false, "Mystery carrier"),
            (14230, false, "SSTV"),
        ];
        want.sort_by_key(|r| r.0);
        assert_eq!((p.total, favs(&p)), (4, want.clone()));
        match p.favorites.iter().find(|r| matches!(r, FavoriteRow::Off { .. })) {
            Some(FavoriteRow::Off { row }) => assert!(row.favorite.is_some()),
            other => panic!("{other:?}"),
        }
        // the signal heard at the favorite's frequency goes with it
        let mystery = p.favorites.iter().find_map(|r| match r {
            FavoriteRow::Off { row } if row.freq_hz == 6_200_000 => Some(row),
            _ => None,
        });
        assert_eq!(mystery.and_then(|r| r.heard.as_ref()).map(|h| h.best_db), Some(-10));

        // pages
        let p = w.search(&StationQuery { lens: Lens::Favorites, offset: 1, limit: 2, ..Default::default() });
        assert_eq!((p.total, favs(&p)), (4, want[1..3].to_vec()));
    }

    #[test]
    fn favorites_ignore_every_filter_but_the_text() {
        let w = World::new();
        let q = |f: StationQuery| w.search(&StationQuery { lens: Lens::Favorites, ..f });
        let every = StationQuery {
            bands: vec!["13m".into()],
            langs: vec!["E".into()],
            countries: vec!["XYZ".into()],
            regions: vec![crate::country::Region::Oc],
            sources: vec!["mine".into()],
            site: Some(u32::MAX),
            ..Default::default()
        };
        assert_eq!(q(every).total, 4);
        assert_eq!(q(StationQuery { scan: Some("s2".into()), ..Default::default() }).total, 4);
        assert_eq!(q(StationQuery { scan: Some("gone".into()), ..Default::default() }).total, 4);
        // the text: a kHz prefix, a label, a station's name
        assert_eq!(favs(&q(StationQuery { q: "142".into(), ..Default::default() })), [(14230, false, "SSTV")]);
        assert_eq!(
            favs(&q(StationQuery { q: "CARRIER".into(), ..Default::default() })),
            [(6200, false, "Mystery carrier")]
        );
        let a = w.cat.station(0).unwrap();
        let named = q(StationQuery { q: a.name.clone(), ..Default::default() });
        assert!(favs(&named).contains(&(a.freq_hz / 1000, true, a.name.as_str())));
        assert!(q(StationQuery { q: "nothing like it".into(), ..Default::default() }).favorites.is_empty());
    }

    #[test]
    fn two_favorites_of_one_station_are_two_rows() {
        let mut w = World::new();
        let a = w.cat.station(0).unwrap();
        let req = FavoriteRequest {
            freq_hz: a.freq_hz,
            mode: Mode::USB,
            label: Some("Evening".into()),
            station: Some(a.station_key()),
        };
        add(&mut w.favorites, &req, &w.cat, 2).unwrap();
        let p = w.search(&StationQuery { lens: Lens::Favorites, ..Default::default() });
        let of_a: Vec<(&str, Mode, u32)> = p
            .favorites
            .iter()
            .filter_map(|r| match r {
                FavoriteRow::Station { row, favorite } if row.id == a.id => {
                    Some((favorite.id.as_str(), favorite.mode, row.id))
                }
                _ => None,
            })
            .collect();
        assert_eq!(of_a.len(), 2);
        assert_ne!(of_a[0].0, of_a[1].0);
        assert_eq!((of_a[0].1, of_a[1].1), (Mode::AM, Mode::USB), "by frequency, then when added");
    }

    #[test]
    fn a_scan_filter_keeps_what_it_heard_and_missed() {
        let w = World::new();
        let s1 = Some("s1".to_string());
        let p = w.search(&StationQuery { scan: s1.clone(), ..Default::default() });
        assert_eq!(ids(&p), [0, 3]);
        assert_eq!(p.scan_db.iter().map(|(&k, &v)| (k, v)).collect::<Vec<_>>(), [(0, -12)], "3 was missed");
        assert_eq!(off(&p), [(5905, "", false, true)]);
        // with a lens: the intersection
        let p = w.search(&StationQuery { scan: s1.clone(), lens: Lens::Listened, ..Default::default() });
        assert_eq!((ids(&p), off(&p)), (vec![3], vec![(5905, "", false, true)]));
        // the favorite heard by s2 marks its signal
        let p = w.search(&StationQuery { scan: Some("s2".into()), ..Default::default() });
        assert_eq!(off(&p), [(6200, "Mystery carrier", true, true)]);
        // a deleted scan keeps nothing
        let p = w.search(&StationQuery { scan: Some("gone".into()), ..Default::default() });
        assert!(p.total == 0 && p.off_schedule.is_empty());
    }
}
