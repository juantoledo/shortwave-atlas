//! The station catalog built from EiBi: one row per (frequency, station, transmitter site)
//! with its schedule slots, plus the lookups the UI needs: candidates for a frequency,
//! filtered and paged search, per-site on-air counts for the globe, and the code tables.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::calendar::{day_of, iso_date, Season};
use crate::country::{country, Region};
use crate::eibi::names::{band_of, bands, lang_name, target, target_region, Band};
use crate::eibi::{ParseReport, Parsed};
use crate::geo::{dist_bearing, sun_at, LatLon, Route, SunAt};
use crate::rig::Mode;
use crate::schedule::{air_status_at, any_on, slot_on, today_parts, AirStatus, Slot, Window};
use crate::text::fold;

/// Default match tolerance around the tuned frequency.
pub const LOOKUP_TOLERANCE_HZ: u32 = 1_000;
/// Rows per search page when the query asks for none, and the most it may ask for.
pub const DEFAULT_LIMIT: u32 = 100;
pub const MAX_LIMIT: u32 = 200;

/// A transmitter site.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Site {
    pub id: u32,
    /// Country the site is in (EiBi code).
    pub itu: String,
    /// flag-icons key, `None` = no flag.
    pub flag: Option<String>,
    /// EiBi site code within the country, if any.
    pub code: Option<String>,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// `false`: EiBi does not say where; shown at a known site of the country.
    pub precise: bool,
}

impl Site {
    pub fn pos(&self) -> LatLon {
        LatLon { lat: self.lat, lon: self.lon }
    }
}

/// A station on one frequency from one site, with every scheduled slot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Station {
    /// Row id; stable for one data version only.
    pub id: u32,
    pub freq_hz: u32,
    pub name: String,
    /// The station's own country (EiBi code), not necessarily the site's.
    pub itu: String,
    pub flag: Option<String>,
    pub mode: Mode,
    /// `Site::id`.
    pub site: u32,
    pub slots: Vec<Slot>,
}

/// A station as seen from a QTH at a given time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Candidate {
    pub station: Station,
    pub site: Site,
    pub route: Route,
    pub air: AirStatus,
    /// Index in `station.slots` of the slot on the air now, else of the next one to start.
    pub slot: Option<u32>,
    /// Today's (UTC) windows, for the 24 h bar.
    pub today: Vec<Window>,
    /// The Sun at the transmitter.
    pub sun: SunAt,
}

/// One row of the station list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationRow {
    pub id: u32,
    pub freq_hz: u32,
    pub name: String,
    pub itu: String,
    pub flag: Option<String>,
    pub mode: Mode,
    pub site: u32,
    pub site_name: String,
    pub site_itu: String,
    pub site_flag: Option<String>,
    pub precise: bool,
    /// On the air now.
    pub on: bool,
    /// Language and target of the slot that matched (the one on the air if any).
    pub lang: String,
    pub target: String,
}

/// Station list filters. Values inside one filter are alternatives (OR); filters combine
/// with AND. Language, region and "on the air" must hold for the same slot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct StationQuery {
    /// Free text (names, countries, languages, targets), or digits = a kHz prefix.
    pub q: String,
    pub on_air: bool,
    /// Band ids (`49m`, or `oob` for out of band).
    pub bands: Vec<String>,
    /// EiBi language codes.
    pub langs: Vec<String>,
    /// Station countries (EiBi codes).
    pub countries: Vec<String>,
    /// Target regions.
    pub regions: Vec<Region>,
    pub site: Option<u32>,
    pub offset: u32,
    /// 0 = `DEFAULT_LIMIT`; capped at `MAX_LIMIT`.
    pub limit: u32,
    /// Also count matches per filter value.
    pub facets: bool,
}

/// How many rows each filter value would give (with that filter itself left out).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Facets {
    pub bands: BTreeMap<String, u32>,
    pub langs: BTreeMap<String, u32>,
    pub countries: BTreeMap<String, u32>,
    pub regions: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationPage {
    /// Rows matching, before paging.
    pub total: u32,
    /// Of those, on the air now.
    pub on_air: u32,
    pub items: Vec<StationRow>,
    pub facets: Option<Facets>,
}

/// A transmitter site on the globe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SiteDot {
    pub id: u32,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub itu: String,
    pub flag: Option<String>,
    pub precise: bool,
    /// Rows on the air now / all rows at this site.
    pub on: u32,
    pub total: u32,
}

/// What is on the air now: every site, and the frequencies in use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Overview {
    pub sites: Vec<SiteDot>,
    /// Frequencies with something on the air, sorted.
    pub on_hz: Vec<u32>,
}

/// Where the data comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DataSource {
    pub name: String,
    pub url: String,
    pub file: String,
    /// `A26`
    pub season: Option<String>,
    /// Season dates, `YYYY-MM-DD`.
    pub from: Option<String>,
    pub to: Option<String>,
    /// Whether the season is the one in force now.
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LangInfo {
    pub code: String,
    pub en: String,
    pub es: Option<String>,
    /// ISO 639-3, for localising names the tables lack.
    pub iso: Option<String>,
    /// Rows broadcasting in it.
    pub rows: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CountryInfo {
    pub itu: String,
    pub flag: Option<String>,
    pub en: String,
    pub es: String,
    pub region: Region,
    /// Rows of stations from this country.
    pub rows: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TargetInfo {
    pub code: String,
    pub en: String,
    pub es: String,
    pub region: Region,
}

/// Code tables and data facts for the UI (loaded once).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CatalogMeta {
    pub source: DataSource,
    pub report: ParseReport,
    pub langs: Vec<LangInfo>,
    pub countries: Vec<CountryInfo>,
    pub targets: Vec<TargetInfo>,
    pub bands: Vec<Band>,
    /// Every frequency in the catalog, sorted.
    pub freqs_hz: Vec<u32>,
    pub stations: u32,
    pub sites: u32,
}

struct Row {
    station: Station,
    band: &'static str,
    /// Folded search text.
    hay: String,
    /// Per slot: its language codes and target region.
    slot_langs: Vec<Vec<String>>,
    slot_regions: Vec<Region>,
}

/// The in-memory catalog.
pub struct Catalog {
    rows: Vec<Row>,
    sites: Vec<Site>,
    langs: Vec<LangInfo>,
    countries: Vec<CountryInfo>,
    targets: Vec<TargetInfo>,
    season: Option<Season>,
    report: ParseReport,
}

/// `S,Q` -> `["S", "Q"]`
fn lang_parts(lang: &str) -> Vec<String> {
    lang.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

fn mode_for(_name: &str) -> Mode {
    // EiBi has no mode column; broadcasts on shortwave are AM (DRM rows would need a marker)
    Mode::AM
}

fn country_names(itu: &str) -> (String, String) {
    match country(itu) {
        Some(c) => (c.en.to_string(), c.es.to_string()),
        None => (itu.to_string(), itu.to_string()),
    }
}

impl Catalog {
    pub fn from_eibi(p: Parsed) -> Self {
        let sites: Vec<Site> = p
            .sites
            .iter()
            .enumerate()
            .map(|(i, s)| Site {
                id: i as u32,
                itu: s.itu.clone(),
                flag: country(&s.itu).and_then(|c| c.flag).map(str::to_string),
                code: s.code.clone(),
                name: s.name.clone(),
                lat: s.lat,
                lon: s.lon,
                precise: s.precise,
            })
            .collect();

        // language and target names
        let mut lang_rows: BTreeMap<String, u32> = BTreeMap::new();
        let mut targets_used: BTreeSet<String> = BTreeSet::new();
        for e in &p.entries {
            for l in lang_parts(&e.slot.lang) {
                *lang_rows.entry(l).or_default() += 1;
            }
            if !e.slot.target.is_empty() {
                targets_used.insert(e.slot.target.clone());
            }
        }
        let langs: Vec<LangInfo> = lang_rows
            .into_iter()
            .map(|(code, rows)| {
                let readme = p.readme.langs.get(&code);
                let tbl = lang_name(&code);
                LangInfo {
                    en: tbl
                        .and_then(|t| t.en)
                        .map(str::to_string)
                        .or_else(|| readme.map(|r| r.name.clone()))
                        .unwrap_or_else(|| code.clone()),
                    es: tbl.map(|t| t.es.to_string()),
                    iso: readme.and_then(|r| r.iso.clone()),
                    code,
                    rows,
                }
            })
            .collect();
        let targets: Vec<TargetInfo> = targets_used
            .into_iter()
            .map(|code| {
                let (en, es) = match target(&code) {
                    Some(t) => (t.en.to_string(), t.es.to_string()),
                    None if country(&code).is_some() => country_names(&code),
                    None => {
                        let en = p.readme.targets.get(&code).cloned().unwrap_or_else(|| code.clone());
                        (en.clone(), en)
                    }
                };
                TargetInfo { region: target_region(&code), code, en, es }
            })
            .collect();
        let lang_ix: HashMap<&str, &LangInfo> = langs.iter().map(|l| (l.code.as_str(), l)).collect();
        let target_ix: HashMap<&str, &TargetInfo> = targets.iter().map(|t| (t.code.as_str(), t)).collect();

        // group entries into rows
        let mut groups: BTreeMap<(u32, u32, u32), Vec<Slot>> = BTreeMap::new();
        for e in p.entries {
            groups.entry((e.freq_hz, e.station, e.site)).or_default().push(e.slot);
        }
        let mut rows: Vec<Row> = groups
            .into_iter()
            .map(|((freq_hz, st, site), mut slots)| {
                slots.sort_by_key(|s| (s.start, s.end));
                let (name, itu) = p.stations[st as usize].clone();
                let site_rec = &sites[site as usize];
                let mut words: Vec<String> =
                    vec![name.clone(), itu.clone(), site_rec.name.clone(), site_rec.itu.clone()];
                for c in [&itu, &site_rec.itu] {
                    let (en, es) = country_names(c);
                    words.extend([en, es]);
                }
                let slot_langs: Vec<Vec<String>> = slots.iter().map(|s| lang_parts(&s.lang)).collect();
                for l in slot_langs.iter().flatten() {
                    if let Some(info) = lang_ix.get(l.as_str()) {
                        words.push(info.en.clone());
                        words.extend(info.es.clone());
                    }
                }
                for s in &slots {
                    if let Some(t) = target_ix.get(s.target.as_str()) {
                        words.extend([t.code.clone(), t.en.clone(), t.es.clone()]);
                    }
                }
                let slot_regions = slots.iter().map(|s| target_region(&s.target)).collect();
                Row {
                    band: band_of(freq_hz),
                    hay: fold(&words.join(" | ")),
                    slot_langs,
                    slot_regions,
                    station: Station {
                        id: 0,
                        freq_hz,
                        flag: country(&itu).and_then(|c| c.flag).map(str::to_string),
                        mode: mode_for(&name),
                        name,
                        itu,
                        site,
                        slots,
                    },
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            a.station
                .freq_hz
                .cmp(&b.station.freq_hz)
                .then_with(|| a.station.name.cmp(&b.station.name))
                .then_with(|| sites[a.station.site as usize].name.cmp(&sites[b.station.site as usize].name))
        });
        for (i, r) in rows.iter_mut().enumerate() {
            r.station.id = i as u32;
        }

        let mut country_rows: BTreeMap<String, u32> = BTreeMap::new();
        for r in &rows {
            *country_rows.entry(r.station.itu.clone()).or_default() += 1;
        }
        let countries = country_rows
            .into_iter()
            .map(|(itu, rows)| {
                let (en, es) = country_names(&itu);
                CountryInfo {
                    flag: country(&itu).and_then(|c| c.flag).map(str::to_string),
                    region: country(&itu).map_or(Region::Other, |c| c.region),
                    itu,
                    en,
                    es,
                    rows,
                }
            })
            .collect();

        Self { rows, sites, langs, countries, targets, season: p.season, report: p.report }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn site(&self, id: u32) -> Option<&Site> {
        self.sites.get(id as usize)
    }

    pub fn station(&self, id: u32) -> Option<&Station> {
        self.rows.get(id as usize).map(|r| &r.station)
    }

    /// Rows within `tol_hz` of `freq_hz` (rows are sorted by frequency).
    fn near(&self, freq_hz: u32, tol_hz: u32) -> &[Row] {
        let lo = self.rows.partition_point(|r| r.station.freq_hz < freq_hz.saturating_sub(tol_hz));
        let hi = self.rows.partition_point(|r| r.station.freq_hz <= freq_hz.saturating_add(tol_hz));
        &self.rows[lo..hi]
    }

    fn candidate(&self, st: &Station, unix: i64, qth: LatLon) -> Candidate {
        let site = self.sites[st.site as usize].clone();
        let slot = st.slots.iter().position(|s| slot_on(s, unix)).or_else(|| {
            st.slots
                .iter()
                .enumerate()
                .filter_map(|(i, s)| air_status_at(std::slice::from_ref(s), unix).minutes.map(|m| (m, i)))
                .min()
                .map(|(_, i)| i)
        });
        Candidate {
            route: dist_bearing(qth, site.pos()),
            air: air_status_at(&st.slots, unix),
            slot: slot.map(|i| i as u32),
            today: today_parts(&st.slots, unix),
            sun: sun_at(site.pos(), unix),
            station: st.clone(),
            site,
        }
    }

    /// Stations near `freq_hz`: on the air first, then the closest to `qth`.
    pub fn lookup(&self, freq_hz: u32, unix: i64, qth: LatLon, tol_hz: u32) -> Vec<Candidate> {
        let mut out: Vec<_> =
            self.near(freq_hz, tol_hz).iter().map(|r| self.candidate(&r.station, unix, qth)).collect();
        out.sort_by(|a, b| b.air.on.cmp(&a.air.on).then(a.route.km.total_cmp(&b.route.km)));
        out
    }

    /// The route to the closest station on the air near `freq_hz` (cheap: no schedule search).
    pub fn strongest_on_air(&self, freq_hz: u32, unix: i64, qth: LatLon, tol_hz: u32) -> Option<Route> {
        self.near(freq_hz, tol_hz)
            .iter()
            .filter(|r| any_on(&r.station.slots, unix))
            .map(|r| dist_bearing(qth, self.sites[r.station.site as usize].pos()))
            .min_by(|a, b| a.km.total_cmp(&b.km))
    }

    /// Filtered, paged station list.
    pub fn search(&self, q: &StationQuery, unix: i64) -> StationPage {
        let text = fold(q.q.trim());
        let terms: Vec<&str> = text.split_whitespace().collect();
        let khz_prefix = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit() || b == b'.');
        let limit = if q.limit == 0 { DEFAULT_LIMIT } else { q.limit.min(MAX_LIMIT) };

        let text_ok = |r: &Row| {
            if khz_prefix {
                let khz = r.station.freq_hz as f64 / 1000.0;
                format!("{khz}").starts_with(&text)
            } else {
                terms.iter().all(|t| r.hay.contains(t))
            }
        };
        // per slot: (language matches, region matches, on the air)
        let slot_tests = |r: &Row, i: usize| -> (bool, bool, bool) {
            let lang = q.langs.is_empty() || r.slot_langs[i].iter().any(|l| q.langs.contains(l));
            let region = q.regions.is_empty() || q.regions.contains(&r.slot_regions[i]);
            let on = !q.on_air || slot_on(&r.station.slots[i], unix);
            (lang, region, on)
        };

        let mut total = 0;
        let mut on_air = 0;
        let mut items = Vec::new();
        let mut facets = Facets::default();
        for r in &self.rows {
            let base = q.site.is_none_or(|s| r.station.site == s) && text_ok(r);
            if !base {
                continue;
            }
            let band_ok = q.bands.is_empty() || q.bands.iter().any(|b| b == r.band);
            let country_ok = q.countries.is_empty() || q.countries.contains(&r.station.itu);
            let tests: Vec<(bool, bool, bool)> = (0..r.station.slots.len()).map(|i| slot_tests(r, i)).collect();
            let slot_ok = tests.iter().any(|&(l, g, o)| l && g && o);

            if q.facets {
                if country_ok && slot_ok {
                    *facets.bands.entry(r.band.to_string()).or_default() += 1;
                }
                if band_ok && slot_ok {
                    *facets.countries.entry(r.station.itu.clone()).or_default() += 1;
                }
                if band_ok && country_ok {
                    let langs: BTreeSet<&String> = tests
                        .iter()
                        .enumerate()
                        .filter(|(_, &(_, g, o))| g && o)
                        .flat_map(|(i, _)| r.slot_langs[i].iter())
                        .collect();
                    for l in langs {
                        *facets.langs.entry(l.clone()).or_default() += 1;
                    }
                    let regions: BTreeSet<Region> = tests
                        .iter()
                        .enumerate()
                        .filter(|(_, &(l, _, o))| l && o)
                        .map(|(i, _)| r.slot_regions[i])
                        .collect();
                    for g in regions {
                        *facets.regions.entry(g.id().to_string()).or_default() += 1;
                    }
                }
            }

            if !(band_ok && country_ok && slot_ok) {
                continue;
            }
            let now: Vec<bool> = r.station.slots.iter().map(|s| slot_on(s, unix)).collect();
            let on = now.iter().any(|&b| b);
            total += 1;
            on_air += u32::from(on);
            if total > q.offset && items.len() < limit as usize {
                // the matching slot, preferring one on the air
                let pick = (0..tests.len())
                    .filter(|&i| tests[i].0 && tests[i].1 && tests[i].2)
                    .max_by_key(|&i| (now[i], std::cmp::Reverse(i)))
                    .unwrap_or(0);
                items.push(self.row(r, on, pick));
            }
        }
        StationPage { total, on_air, items, facets: q.facets.then_some(facets) }
    }

    fn row(&self, r: &Row, on: bool, slot: usize) -> StationRow {
        let st = &r.station;
        let site = &self.sites[st.site as usize];
        let s = st.slots.get(slot);
        StationRow {
            id: st.id,
            freq_hz: st.freq_hz,
            name: st.name.clone(),
            itu: st.itu.clone(),
            flag: st.flag.clone(),
            mode: st.mode,
            site: site.id,
            site_name: site.name.clone(),
            site_itu: site.itu.clone(),
            site_flag: site.flag.clone(),
            precise: site.precise,
            on,
            lang: s.map(|s| s.lang.clone()).unwrap_or_default(),
            target: s.map(|s| s.target.clone()).unwrap_or_default(),
        }
    }

    /// Every site with its on-air count, and the frequencies on the air.
    pub fn overview(&self, unix: i64) -> Overview {
        let mut on = vec![0u32; self.sites.len()];
        let mut total = vec![0u32; self.sites.len()];
        let mut on_hz = Vec::new();
        for r in &self.rows {
            let i = r.station.site as usize;
            total[i] += 1;
            if any_on(&r.station.slots, unix) {
                on[i] += 1;
                on_hz.push(r.station.freq_hz);
            }
        }
        on_hz.dedup();
        let sites = self
            .sites
            .iter()
            .map(|s| SiteDot {
                id: s.id,
                lat: s.lat,
                lon: s.lon,
                name: s.name.clone(),
                itu: s.itu.clone(),
                flag: s.flag.clone(),
                precise: s.precise,
                on: on[s.id as usize],
                total: total[s.id as usize],
            })
            .collect();
        Overview { sites, on_hz }
    }

    /// Code tables, the import report and the data season.
    pub fn meta(&self, unix: i64) -> CatalogMeta {
        let range = self.season.map(|s| s.range());
        let mut freqs_hz: Vec<u32> = self.rows.iter().map(|r| r.station.freq_hz).collect();
        freqs_hz.dedup();
        CatalogMeta {
            source: DataSource {
                name: "EiBi".into(),
                url: "http://www.eibispace.de/dx/".into(),
                file: self.report.source_file.clone(),
                season: self.season.map(|s| s.code()),
                from: range.map(|r| iso_date(day_of(r.0))),
                to: range.map(|r| iso_date(day_of(r.1) - 1)),
                current: self.season.is_some_and(|s| s == Season::at(unix)),
            },
            report: self.report.clone(),
            langs: self.langs.clone(),
            countries: self.countries.clone(),
            targets: self.targets.clone(),
            bands: bands(),
            freqs_hz,
            stations: self.rows.len() as u32,
            sites: self.sites.len() as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::{days_from_civil, SECS_PER_DAY};
    use crate::eibi::parse;
    use crate::eibi::tests::files;

    const SANTIAGO: LatLon = LatLon { lat: -33.45, lon: -70.67 };

    /// Thursday 2026-10-08 at `h:m` UTC.
    fn at(h: i64, m: i64) -> i64 {
        i64::from(days_from_civil(2026, 10, 8)) * SECS_PER_DAY + h * 3600 + m * 60
    }

    fn catalog() -> Catalog {
        Catalog::from_eibi(parse(&files()).unwrap())
    }

    fn names(rows: &[StationRow]) -> Vec<(u32, &str)> {
        rows.iter().map(|r| (r.freq_hz / 1000, r.name.as_str())).collect()
    }

    fn query(f: impl FnOnce(&mut StationQuery)) -> StationQuery {
        let mut q = StationQuery::default();
        f(&mut q);
        q
    }

    #[test]
    fn rows_group_by_frequency_station_and_site() {
        let c = catalog();
        assert_eq!(c.len(), 10);
        let ids: Vec<u32> = (0..c.len() as u32).map(|i| c.station(i).unwrap().id).collect();
        assert_eq!(ids, (0..10).collect::<Vec<_>>());
        let freqs: Vec<u32> = (0..10).map(|i| c.station(i).unwrap().freq_hz / 1000).collect();
        assert_eq!(freqs, [5900, 5910, 5915, 6070, 7350, 9002, 9410, 9411, 11000, 11010]);
        let bbc = c.station(6).unwrap();
        assert_eq!(
            (bbc.name.as_str(), bbc.itu.as_str(), bbc.flag.as_deref(), bbc.mode),
            ("BBC", "G", Some("gb"), Mode::AM)
        );
        assert_eq!(c.site(bbc.site).unwrap().flag.as_deref(), Some("za"));
    }

    #[test]
    fn lookup_puts_on_air_first() {
        let c = catalog();
        // 6070 CNR is on 24 h; 5915 CRI 16-17 UTC is off at 10:00
        let l = c.lookup(6_070_000, at(10, 0), SANTIAGO, LOOKUP_TOLERANCE_HZ);
        assert_eq!(l.len(), 1);
        assert!(l[0].air.on && l[0].air.minutes.is_none());
        assert_eq!(l[0].today, vec![[0, 1440]]);
        assert_eq!(l[0].slot, Some(0));
        let wide = c.lookup(5_910_000, at(14, 45), SANTIAGO, 10_000);
        // then by distance: Kashi is closer to Santiago than Goyang
        assert_eq!(wide.iter().map(|c| c.station.freq_hz / 1000).collect::<Vec<_>>(), [5910, 5915, 5900]);
        assert!(wide[0].air.on && !wide[1].air.on);
        assert!(c.lookup(6_072_000, at(10, 0), SANTIAGO, LOOKUP_TOLERANCE_HZ).is_empty());
    }

    #[test]
    fn candidate_points_at_the_next_slot_when_off() {
        let c = catalog();
        // 5900 New Korea Hope: Tu, Th, Su 17:00-18:00; Thursday 10:00 -> later today
        let k = &c.lookup(5_900_000, at(10, 0), SANTIAGO, LOOKUP_TOLERANCE_HZ)[0];
        assert!(!k.air.on);
        assert_eq!((k.air.minutes, k.air.edge, k.slot), (Some(420), 1020, Some(0)));
        assert_eq!(k.today, vec![[1020, 1080]]);
    }

    #[test]
    fn strongest_on_air_skips_off_air_rows() {
        let c = catalog();
        assert!(c.strongest_on_air(6_070_000, at(10, 0), SANTIAGO, LOOKUP_TOLERANCE_HZ).is_some());
        assert!(c.strongest_on_air(5_915_000, at(10, 0), SANTIAGO, LOOKUP_TOLERANCE_HZ).is_none());
        assert!(c.strongest_on_air(5_915_000, at(16, 30), SANTIAGO, LOOKUP_TOLERANCE_HZ).is_some());
    }

    #[test]
    fn search_text_digits_and_accents() {
        let c = catalog();
        let s = |text: &str| c.search(&query(|q| q.q = text.into()), at(10, 0));
        assert_eq!(names(&s("bbc").items), [(9410, "BBC"), (9411, "BBC")]);
        assert_eq!(
            names(&s("59").items),
            [(5900, "New Korea Hope Bcing"), (5910, "Radio Bulgaria"), (5915, "China Radio Int.")]
        );
        // country names in English and Spanish, folded
        assert_eq!(s("reino unido").total, 2);
        assert_eq!(s("Sudáfrica").total, 2); // BBC via South Africa
        assert_eq!(s("mandarin").total, 1);
        assert_eq!(s("mandarín").total, 1);
        // target names, and several words
        assert_eq!(names(&s("centroamerica").items), [(7350, "Radio Free Test")]);
        assert_eq!(s("bbc europe").total, 2);
        assert_eq!(s("bbc asia").total, 0);
        assert_eq!(s("").total, 10);
    }

    #[test]
    fn search_filters_combine() {
        let c = catalog();
        let t = at(14, 45);
        let s = |f: fn(&mut StationQuery)| c.search(&query(f), t);
        assert_eq!(names(&s(|q| q.on_air = true).items), [(5910, "Radio Bulgaria"), (6070, "China Radio Int.")]);
        assert_eq!(s(|q| q.bands = vec!["49m".into()]).total, 4);
        assert_eq!(s(|q| q.bands = vec!["oob".into(), "41m".into()]).total, 4); // 9002, 11000, 11010 + 7350
        assert_eq!(s(|q| q.countries = vec!["KOR".into()]).total, 3);
        assert_eq!(s(|q| q.langs = vec!["E".into()]).total, 4);
        assert_eq!(s(|q| q.regions = vec![Region::Am]).total, 2);
        assert_eq!(
            s(|q| {
                q.langs = vec!["E".into()];
                q.on_air = true;
            })
            .total,
            1
        );
        let page = s(|q| q.site = Some(0));
        assert!(page.items.iter().all(|r| r.site == 0));
    }

    #[test]
    fn search_pages() {
        let c = catalog();
        let p = c.search(
            &query(|q| {
                q.limit = 3;
                q.offset = 4;
            }),
            at(14, 45),
        );
        assert_eq!((p.total, p.items.len()), (10, 3));
        assert_eq!(p.items[0].freq_hz / 1000, 7350);
        assert_eq!(p.on_air, 2);
        let all = c.search(&query(|q| q.limit = 10_000), at(14, 45));
        assert_eq!(all.items.len(), 10);
    }

    #[test]
    fn facets_leave_their_own_filter_out() {
        let c = catalog();
        let p = c.search(
            &query(|q| {
                q.countries = vec!["KOR".into()];
                q.facets = true;
            }),
            at(14, 45),
        );
        let f = p.facets.unwrap();
        // countries ignore the country filter
        assert_eq!(f.countries.get("G"), Some(&2));
        assert_eq!(f.countries.get("KOR"), Some(&3));
        // the others count Korean rows only
        assert_eq!(f.langs.get("K"), Some(&3));
        assert_eq!(f.langs.get("E"), None);
        assert_eq!(f.bands.values().sum::<u32>(), 3);
        assert!(c.search(&StationQuery::default(), at(0, 0)).facets.is_none());
    }

    #[test]
    fn rows_show_the_matching_slot() {
        let c = catalog();
        let p = c.search(&query(|q| q.q = "7350".into()), at(14, 45));
        let r = &p.items[0];
        assert_eq!(
            (r.lang.as_str(), r.target.as_str(), r.site_itu.as_str(), r.site_flag.as_deref()),
            ("S", "CAm", "BUL", Some("bg"))
        );
        assert_eq!((r.itu.as_str(), r.flag.as_deref()), ("CLA", None));
    }

    #[test]
    fn overview_counts_sites_on_air() {
        let c = catalog();
        let o = c.overview(at(14, 45));
        assert_eq!(o.sites.len(), c.sites.len());
        assert_eq!(o.sites.iter().map(|s| s.total).sum::<u32>(), 10);
        assert_eq!(o.sites.iter().map(|s| s.on).sum::<u32>(), 2);
        assert_eq!(o.on_hz, [5_910_000, 6_070_000]);
    }

    #[test]
    fn meta_lists_codes_in_use() {
        let c = catalog();
        let m = c.meta(at(10, 0));
        assert_eq!(m.source.season.as_deref(), Some("A26"));
        assert_eq!((m.source.from.as_deref(), m.source.to.as_deref()), (Some("2026-03-29"), Some("2026-10-24")));
        assert!(m.source.current);
        assert!(!c.meta(at(10, 0) + 30 * SECS_PER_DAY).source.current);
        let spanish = m.langs.iter().find(|l| l.code == "S").unwrap();
        assert_eq!((spanish.en.as_str(), spanish.es.as_deref(), spanish.rows), ("Spanish", Some("Español"), 1));
        let hindi = m.langs.iter().find(|l| l.code == "HI").unwrap();
        assert_eq!((hindi.es.as_deref(), hindi.iso.as_deref()), (Some("Hindi"), None));
        let g = m.countries.iter().find(|c| c.itu == "G").unwrap();
        assert_eq!(
            (g.en.as_str(), g.es.as_str(), g.flag.as_deref(), g.rows),
            ("United Kingdom", "Reino Unido", Some("gb"), 2)
        );
        let cam = m.targets.iter().find(|t| t.code == "CAm").unwrap();
        assert_eq!((cam.es.as_str(), cam.region), ("Centroamérica", Region::Am));
        let kre = m.targets.iter().find(|t| t.code == "KRE").unwrap();
        assert_eq!(kre.en, "North Korea");
        assert_eq!(m.freqs_hz.len(), 10);
        assert_eq!((m.stations, m.sites), (10, c.sites.len() as u32));
        assert_eq!(m.bands.len(), 14);
    }
}
