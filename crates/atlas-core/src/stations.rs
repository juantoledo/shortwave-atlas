//! The station catalog built from the data sources: one row per record (frequency, station,
//! transmitter site) with its schedule slots, plus the lookups the UI needs: candidates for a
//! frequency, filtered and paged search, per-site on-air counts for the globe, and the code
//! tables. `CatalogCell` holds the running one, replaced whole when a source changes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::country::{country, Region, COUNTRIES};
use crate::eibi::names::{band_of, bands, lang_name, target, target_region, Band, LANGS, TARGETS};
use crate::eibi::{Parsed, SiteKey, SiteRec};
use crate::geo::{dist_bearing, sun_at, LatLon, Route, SunAt};
use crate::lens::{HeardRow, OffSchedule};
use crate::rig::Mode;
use crate::schedule::{air_status_at, any_on, slot_on, today_parts, AirStatus, Slot, Window};
use crate::source::{
    Gazetteer, SiteSpec, SourceData, SourceDesc, SourceInfo, SourceKind, SourceStamp, StationKey, Vocabulary,
};
use crate::text::fold;

/// Default match tolerance around the tuned frequency.
pub const LOOKUP_TOLERANCE_HZ: u32 = 1_000;
/// Rows per search page when the query asks for none, and the most it may ask for.
pub const DEFAULT_LIMIT: u32 = 100;
pub const MAX_LIMIT: u32 = 200;

/// Rows per page for `q`.
pub(crate) fn page_limit(q: &StationQuery) -> u32 {
    if q.limit == 0 {
        DEFAULT_LIMIT
    } else {
        q.limit.min(MAX_LIMIT)
    }
}

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

impl Station {
    pub fn station_key(&self) -> StationKey {
        StationKey { source: self.source.clone(), key: self.key.clone() }
    }
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
    /// `SourceDesc::id` of the source it comes from, and its `Record::key` there.
    pub source: String,
    pub key: String,
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
    /// `SourceDesc::id`, and the `Record::key` there.
    pub source: String,
    pub key: String,
}

/// Which stations the list shows: the whole schedule, those a band scan logged, the
/// favorites, or those in the listening history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Lens {
    #[default]
    All,
    Heard,
    Favorites,
    Listened,
}

/// The order of the Heard lens (the others are by frequency).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Sort {
    #[default]
    Freq,
    /// Best reading first.
    Strongest,
    /// Last heard first.
    Recent,
}

/// Station list filters. Values inside one filter are alternatives (OR); filters combine
/// with AND. Language and region must hold for the same slot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct StationQuery {
    /// Free text (names, countries, languages, targets), or digits = a kHz prefix.
    pub q: String,
    /// Band ids (`49m`, or `oob` for out of band).
    pub bands: Vec<String>,
    /// EiBi language codes.
    pub langs: Vec<String>,
    /// Station countries (EiBi codes).
    pub countries: Vec<String>,
    /// Target regions.
    pub regions: Vec<Region>,
    /// Source ids (`eibi`).
    pub sources: Vec<String>,
    pub site: Option<u32>,
    pub lens: Lens,
    /// Only the stations this saved scan heard or missed.
    pub scan: Option<String>,
    /// The Heard lens's order.
    pub sort: Sort,
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
    pub sources: BTreeMap<String, u32>,
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
    /// Under a scan filter: the reading of each row in `items` the scan heard (the others it
    /// missed).
    pub scan_db: BTreeMap<u32, i16>,
    /// Signals and favorites the schedule has no row for, by frequency (not paged).
    pub off_schedule: Vec<OffSchedule>,
    /// The Heard lens instead of `items` and `off_schedule`: logged stations and unidentified
    /// signals in one list, in `StationQuery::sort` order, paged (`total` counts both).
    pub heard: Vec<HeardRow>,
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
    /// Of those, rows of the listener's own stations.
    pub mine: u32,
}

/// What is on the air now: every site, and the frequencies in use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Overview {
    pub sites: Vec<SiteDot>,
    /// Frequencies with something on the air, sorted.
    pub on_hz: Vec<u32>,
    /// `Catalog::generation`.
    pub generation: u32,
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

/// Code tables and data facts for the UI (loaded again when `generation` changes).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CatalogMeta {
    /// Where the stations come from, EiBi first.
    pub sources: Vec<SourceInfo>,
    /// `Catalog::generation`.
    pub generation: u32,
    pub langs: Vec<LangInfo>,
    pub countries: Vec<CountryInfo>,
    pub targets: Vec<TargetInfo>,
    pub bands: Vec<Band>,
    /// Every frequency in the catalog, sorted.
    pub freqs_hz: Vec<u32>,
    pub stations: u32,
    pub sites: u32,
}

/// Every code a station may use and the known transmitter sites, for the station form's pickers
/// (`Call::CodeTables`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CodeTables {
    /// The built-in languages and the vocabulary's; `rows` counts the catalog's.
    pub langs: Vec<LangInfo>,
    /// Every country.
    pub countries: Vec<CountryInfo>,
    /// The built-in target areas and the vocabulary's (countries are targets too).
    pub targets: Vec<TargetInfo>,
    /// The known sites with their own coordinates, by country and name (ids are the catalog's).
    pub sites: Vec<Site>,
    /// Countries a station may stand in without a site: those with a known site to show it at.
    pub placeable: Vec<String>,
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
    sources: Vec<SourceDesc>,
    /// Row id by source id and record key.
    keys: HashMap<String, HashMap<String, u32>>,
    generation: u32,
}

/// `S,Q` -> `["S", "Q"]`
fn lang_parts(lang: &str) -> Vec<String> {
    lang.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

fn country_names(itu: &str) -> (String, String) {
    match country(itu) {
        Some(c) => (c.en.to_string(), c.es.to_string()),
        None => (itu.to_string(), itu.to_string()),
    }
}

/// A language's names: the built-in table's, else the vocabulary's, else its code.
fn lang_info(code: String, vocab: &Vocabulary, rows: u32) -> LangInfo {
    let named = vocab.langs.get(&code);
    let tbl = lang_name(&code);
    LangInfo {
        en: tbl
            .and_then(|t| t.en)
            .map(str::to_string)
            .or_else(|| named.map(|r| r.name.clone()))
            .unwrap_or_else(|| code.clone()),
        es: tbl.map(|t| t.es.to_string()),
        iso: named.and_then(|r| r.iso.clone()),
        code,
        rows,
    }
}

/// A target's names: the built-in table's, a country's, else the vocabulary's.
fn target_info(code: String, vocab: &Vocabulary) -> TargetInfo {
    let (en, es) = match target(&code) {
        Some(t) => (t.en.to_string(), t.es.to_string()),
        None if country(&code).is_some() => country_names(&code),
        None => {
            let en = vocab.targets.get(&code).cloned().unwrap_or_else(|| code.clone());
            (en.clone(), en)
        }
    };
    TargetInfo { region: target_region(&code), code, en, es }
}

fn country_info(itu: String, rows: u32) -> CountryInfo {
    let (en, es) = country_names(&itu);
    CountryInfo {
        flag: country(&itu).and_then(|c| c.flag).map(str::to_string),
        region: country(&itu).map_or(Region::Other, |c| c.region),
        itu,
        en,
        es,
        rows,
    }
}

fn site_of(id: u32, s: &SiteRec) -> Site {
    Site {
        id,
        itu: s.itu.clone(),
        flag: country(&s.itu).and_then(|c| c.flag).map(str::to_string),
        code: s.code.clone(),
        name: s.name.clone(),
        lat: s.lat,
        lon: s.lon,
        precise: s.precise,
    }
}

impl Catalog {
    /// The catalog of EiBi alone.
    pub fn from_eibi(p: Parsed) -> Self {
        let s = p.into_source();
        Self::build(&[&s.data], &s.vocabulary, &s.gazetteer)
    }

    /// The catalog of every source's records, with the code names and sites they share. The
    /// gazetteer's sites come first and in its order, so their ids hold when another source
    /// changes; sites of a source's own follow.
    pub fn build(sources: &[&SourceData], vocab: &Vocabulary, gaz: &Gazetteer) -> Self {
        let mut sites: Vec<Site> = gaz.sites.iter().enumerate().map(|(i, s)| site_of(i as u32, s)).collect();
        let mut site_ix: HashMap<SiteKey, u32> =
            gaz.sites.iter().enumerate().map(|(i, s)| (s.key(), i as u32)).collect();
        // (source, record, site id)
        let mut placed = Vec::new();
        for (src, data) in sources.iter().enumerate() {
            for rec in &data.records {
                let site = match &rec.site {
                    SiteSpec::Known(i) if (*i as usize) < gaz.sites.len() => *i,
                    SiteSpec::Known(_) => continue,
                    SiteSpec::At(s) => *site_ix.entry(s.key()).or_insert_with(|| {
                        sites.push(site_of(sites.len() as u32, s));
                        (sites.len() - 1) as u32
                    }),
                };
                placed.push((src, rec, site));
            }
        }

        // language and target names
        let mut lang_rows: BTreeMap<String, u32> = BTreeMap::new();
        let mut targets_used: BTreeSet<String> = BTreeSet::new();
        for slot in placed.iter().flat_map(|(_, r, _)| &r.slots) {
            for l in lang_parts(&slot.lang) {
                *lang_rows.entry(l).or_default() += 1;
            }
            if !slot.target.is_empty() {
                targets_used.insert(slot.target.clone());
            }
        }
        let langs: Vec<LangInfo> = lang_rows.into_iter().map(|(code, rows)| lang_info(code, vocab, rows)).collect();
        let targets: Vec<TargetInfo> = targets_used.into_iter().map(|code| target_info(code, vocab)).collect();
        let lang_ix: HashMap<&str, &LangInfo> = langs.iter().map(|l| (l.code.as_str(), l)).collect();
        let target_ix: HashMap<&str, &TargetInfo> = targets.iter().map(|t| (t.code.as_str(), t)).collect();

        let mut rows: Vec<Row> = placed
            .into_iter()
            .map(|(src, rec, site)| {
                let (name, itu, slots) = (rec.name.clone(), rec.itu.clone(), rec.slots.clone());
                let site_rec = &sites[site as usize];
                let mut words: Vec<String> =
                    vec![name.clone(), itu.clone(), site_rec.name.clone(), site_rec.itu.clone()];
                for c in [&itu, &site_rec.itu] {
                    let (en, es) = country_names(c);
                    words.extend([en, es]);
                }
                if !rec.notes.is_empty() {
                    words.push(rec.notes.clone());
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
                    band: band_of(rec.freq_hz),
                    hay: fold(&words.join(" | ")),
                    slot_langs,
                    slot_regions,
                    station: Station {
                        id: 0,
                        freq_hz: rec.freq_hz,
                        flag: country(&itu).and_then(|c| c.flag).map(str::to_string),
                        mode: rec.mode,
                        name,
                        itu,
                        site,
                        slots,
                        source: sources[src].desc.id.clone(),
                        key: rec.key.clone(),
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
        let mut keys: HashMap<String, HashMap<String, u32>> = HashMap::new();
        for (i, r) in rows.iter_mut().enumerate() {
            r.station.id = i as u32;
            keys.entry(r.station.source.clone()).or_default().entry(r.station.key.clone()).or_insert(i as u32);
        }

        let mut country_rows: BTreeMap<String, u32> = BTreeMap::new();
        for r in &rows {
            *country_rows.entry(r.station.itu.clone()).or_default() += 1;
        }
        let countries = country_rows.into_iter().map(|(itu, rows)| country_info(itu, rows)).collect();

        let sources = sources.iter().map(|s| s.desc.clone()).collect();
        Self { rows, sites, langs, countries, targets, sources, keys, generation: 0 }
    }

    /// The code tables for the station form, against the vocabulary and gazetteer this catalog
    /// was built with.
    pub fn code_tables(&self, vocab: &Vocabulary, gaz: &Gazetteer) -> CodeTables {
        let lang_rows: HashMap<&str, u32> = self.langs.iter().map(|l| (l.code.as_str(), l.rows)).collect();
        let codes: BTreeSet<String> =
            LANGS.iter().map(|l| l.code.to_string()).chain(vocab.langs.keys().cloned()).collect();
        let langs = codes
            .into_iter()
            .map(|c| {
                let rows = lang_rows.get(c.as_str()).copied().unwrap_or(0);
                lang_info(c, vocab, rows)
            })
            .collect();
        let country_rows: HashMap<&str, u32> = self.countries.iter().map(|c| (c.itu.as_str(), c.rows)).collect();
        let countries = COUNTRIES
            .iter()
            .map(|c| country_info(c.itu.to_string(), country_rows.get(c.itu).copied().unwrap_or(0)))
            .collect();
        let targets: BTreeSet<String> = TARGETS
            .iter()
            .map(|t| t.code.to_string())
            .chain(vocab.targets.keys().cloned())
            .filter(|c| country(c).is_none())
            .collect();
        let mut sites: Vec<Site> = self.sites.iter().take(gaz.sites.len()).filter(|s| s.precise).cloned().collect();
        sites.sort_by(|a, b| a.itu.cmp(&b.itu).then_with(|| a.name.cmp(&b.name)));
        let placeable = COUNTRIES
            .iter()
            .map(|c| c.itu)
            .filter(|itu| gaz.first_site.contains_key(*itu) || gaz.sites.iter().any(|s| s.itu == *itu && !s.precise))
            .map(str::to_string)
            .collect();
        CodeTables {
            langs,
            countries,
            targets: targets.into_iter().map(|c| target_info(c, vocab)).collect(),
            sites,
            placeable,
        }
    }

    /// Changes whenever the catalog is built again (`CatalogCell::replace`): station ids from
    /// another generation mean other stations.
    pub fn generation(&self) -> u32 {
        self.generation
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

    /// Every station, by frequency.
    pub fn stations(&self) -> impl Iterator<Item = &Station> {
        self.rows.iter().map(|r| &r.station)
    }

    /// Stations from `lo_hz` to `hi_hz`, by frequency.
    pub fn between(&self, lo_hz: u32, hi_hz: u32) -> impl Iterator<Item = &Station> {
        let lo = self.rows.partition_point(|r| r.station.freq_hz < lo_hz);
        let hi = self.rows.partition_point(|r| r.station.freq_hz <= hi_hz);
        self.rows[lo..hi.max(lo)].iter().map(|r| &r.station)
    }

    /// The station with `key` in source `source`.
    pub fn by_key(&self, source: &str, key: &str) -> Option<&Station> {
        let id = *self.keys.get(source)?.get(key)?;
        self.station(id)
    }

    /// Which versions of the sources this catalog was built from.
    pub fn stamps(&self) -> Vec<SourceStamp> {
        self.sources.iter().map(SourceDesc::stamp).collect()
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
        self.search_in(q, unix, None)
    }

    /// `search`, where a lens other than `All` or a scan filter keeps only the station ids in
    /// `only` (none without it). `lens::search` works them out.
    pub fn search_in(&self, q: &StationQuery, unix: i64, only: Option<&HashSet<u32>>) -> StationPage {
        let limit = page_limit(q);
        self.rows_where(q, unix, only, |n| n > q.offset && n - q.offset <= limit)
    }

    /// `search_in` without paging: every row that matches.
    pub(crate) fn matching(&self, q: &StationQuery, unix: i64, only: Option<&HashSet<u32>>) -> StationPage {
        self.rows_where(q, unix, only, |_| true)
    }

    /// The rows that match `q`, building only the `n`th ones (1-based) that `keep` says.
    fn rows_where(
        &self,
        q: &StationQuery,
        unix: i64,
        only: Option<&HashSet<u32>>,
        keep: impl Fn(u32) -> bool,
    ) -> StationPage {
        let restricted = q.lens != Lens::All || q.scan.is_some();
        let text = fold(q.q.trim());
        let terms: Vec<&str> = text.split_whitespace().collect();
        let khz_prefix = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit() || b == b'.');

        let text_ok = |r: &Row| {
            if khz_prefix {
                let khz = r.station.freq_hz as f64 / 1000.0;
                format!("{khz}").starts_with(&text)
            } else {
                terms.iter().all(|t| r.hay.contains(t))
            }
        };
        // per slot: (language matches, region matches)
        let slot_tests = |r: &Row, i: usize| -> (bool, bool) {
            let lang = q.langs.is_empty() || r.slot_langs[i].iter().any(|l| q.langs.contains(l));
            let region = q.regions.is_empty() || q.regions.contains(&r.slot_regions[i]);
            (lang, region)
        };

        let mut total = 0;
        let mut on_air = 0;
        let mut items = Vec::new();
        let mut facets = Facets::default();
        for r in &self.rows {
            let base = q.site.is_none_or(|s| r.station.site == s)
                && (!restricted || only.is_some_and(|l| l.contains(&r.station.id)))
                && text_ok(r);
            if !base {
                continue;
            }
            let band_ok = q.bands.is_empty() || q.bands.iter().any(|b| b == r.band);
            let country_ok = q.countries.is_empty() || q.countries.contains(&r.station.itu);
            let source_ok = q.sources.is_empty() || q.sources.contains(&r.station.source);
            let tests: Vec<(bool, bool)> = (0..r.station.slots.len()).map(|i| slot_tests(r, i)).collect();
            let slot_ok = tests.iter().any(|&(l, g)| l && g);

            if q.facets {
                if country_ok && slot_ok && source_ok {
                    *facets.bands.entry(r.band.to_string()).or_default() += 1;
                }
                if band_ok && slot_ok && source_ok {
                    *facets.countries.entry(r.station.itu.clone()).or_default() += 1;
                }
                if band_ok && country_ok && slot_ok {
                    *facets.sources.entry(r.station.source.clone()).or_default() += 1;
                }
                if band_ok && country_ok && source_ok {
                    let langs: BTreeSet<&String> = tests
                        .iter()
                        .enumerate()
                        .filter(|(_, &(_, g))| g)
                        .flat_map(|(i, _)| r.slot_langs[i].iter())
                        .collect();
                    for l in langs {
                        *facets.langs.entry(l.clone()).or_default() += 1;
                    }
                    let regions: BTreeSet<Region> =
                        tests.iter().enumerate().filter(|(_, &(l, _))| l).map(|(i, _)| r.slot_regions[i]).collect();
                    for g in regions {
                        *facets.regions.entry(g.id().to_string()).or_default() += 1;
                    }
                }
            }

            if !(band_ok && country_ok && slot_ok && source_ok) {
                continue;
            }
            let now: Vec<bool> = r.station.slots.iter().map(|s| slot_on(s, unix)).collect();
            let on = now.iter().any(|&b| b);
            total += 1;
            on_air += u32::from(on);
            if keep(total) {
                // the matching slot, preferring one on the air
                let pick = (0..tests.len())
                    .filter(|&i| tests[i].0 && tests[i].1)
                    .max_by_key(|&i| (now[i], std::cmp::Reverse(i)))
                    .unwrap_or(0);
                items.push(self.row(r, on, pick));
            }
        }
        StationPage {
            total,
            on_air,
            items,
            facets: q.facets.then_some(facets),
            scan_db: BTreeMap::new(),
            off_schedule: vec![],
            heard: vec![],
        }
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
            source: st.source.clone(),
            key: st.key.clone(),
        }
    }

    /// Every site with its on-air count, and the frequencies on the air.
    pub fn overview(&self, unix: i64) -> Overview {
        let mut on = vec![0u32; self.sites.len()];
        let mut total = vec![0u32; self.sites.len()];
        let mut mine = vec![0u32; self.sites.len()];
        let user: HashSet<&str> =
            self.sources.iter().filter(|s| s.kind == SourceKind::User).map(|s| s.id.as_str()).collect();
        let mut on_hz = Vec::new();
        for r in &self.rows {
            let i = r.station.site as usize;
            total[i] += 1;
            mine[i] += u32::from(user.contains(r.station.source.as_str()));
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
                mine: mine[s.id as usize],
            })
            .collect();
        Overview { sites, on_hz, generation: self.generation }
    }

    /// The sources with their import reports and seasons, and the code tables.
    pub fn meta(&self, unix: i64) -> CatalogMeta {
        let mut freqs_hz: Vec<u32> = self.rows.iter().map(|r| r.station.freq_hz).collect();
        freqs_hz.dedup();
        let rows_of = |id: &str| self.rows.iter().filter(|r| r.station.source == id).count() as u32;
        CatalogMeta {
            sources: self.sources.iter().map(|s| s.info(rows_of(&s.id), unix)).collect(),
            generation: self.generation,
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

/// The running catalog, replaced whole when a source changes. Readers take a snapshot (`get`)
/// and keep to it for one request, so station ids within it hold.
pub struct CatalogCell(RwLock<Arc<Catalog>>);

impl CatalogCell {
    pub fn new(catalog: Catalog) -> Arc<Self> {
        Arc::new(Self(RwLock::new(Arc::new(catalog))))
    }

    pub fn get(&self) -> Arc<Catalog> {
        self.0.read().expect("catalog lock").clone()
    }

    /// Put `catalog` in, as the next generation.
    pub fn replace(&self, mut catalog: Catalog) {
        let mut cur = self.0.write().expect("catalog lock");
        catalog.generation = cur.generation.wrapping_add(1);
        *cur = Arc::new(catalog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::{days_from_civil, SECS_PER_DAY};
    use crate::eibi::parse;
    use crate::eibi::tests::files;
    use crate::schedule::{Days, SeasonOnly};
    use crate::source::{Record, SourceKind};

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
        assert_eq!(s(|q| q.bands = vec!["49m".into()]).total, 4);
        assert_eq!(s(|q| q.bands = vec!["oob".into(), "41m".into()]).total, 4); // 9002, 11000, 11010 + 7350
        assert_eq!(s(|q| q.countries = vec!["KOR".into()]).total, 3);
        assert_eq!(s(|q| q.langs = vec!["E".into()]).total, 4);
        assert_eq!(s(|q| q.regions = vec![Region::Am]).total, 2);
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
        // unpaged, for the Heard lens
        let p = c.matching(&query(|q| q.limit = 3), at(14, 45), None);
        assert_eq!((p.total, p.items.len(), p.on_air), (10, 10, 2));
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
        let [eibi] = &m.sources[..] else { panic!("{:?}", m.sources) };
        assert_eq!((eibi.id.as_str(), eibi.version.as_str(), eibi.rows), ("eibi", "sked-a26.csv", 10));
        assert_eq!(eibi.season.as_deref(), Some("A26"));
        assert_eq!((eibi.from.as_deref(), eibi.to.as_deref()), (Some("2026-03-29"), Some("2026-10-24")));
        assert!(eibi.current && !eibi.editable);
        assert_eq!(eibi.report.as_ref().map(|r| r.kept), Some(10));
        assert!(!c.meta(at(10, 0) + 30 * SECS_PER_DAY).sources[0].current);
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

    /// The fixture's catalog alone, and with a second source: a station on 6075 kHz from a site
    /// of its own, one at an EiBi site, another at the same site of its own, and one at a site
    /// the gazetteer does not have.
    fn two_sources() -> (Catalog, Catalog) {
        let eibi = parse(&files()).unwrap().into_source();
        let slot = Slot {
            start: 0,
            end: 1440,
            days: Days::Daily,
            lang: "S".into(),
            target: "SAm".into(),
            season: SeasonOnly::All,
            from: None,
            to: None,
            heard: None,
        };
        let rec = |key: &str, khz: u32, name: &str, site| Record {
            key: key.into(),
            freq_hz: khz * 1000,
            mode: Mode::USB,
            name: name.into(),
            itu: "CHL".into(),
            site,
            slots: vec![slot.clone()],
            notes: if key == "a" { "heard on the ferry".into() } else { String::new() },
        };
        let own =
            SiteRec { itu: "CHL".into(), code: None, name: "Santiago".into(), lat: -33.45, lon: -70.67, precise: true };
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
                rec("a", 6075, "Radio Local", SiteSpec::At(own.clone())),
                rec("b", 6070, "Radio Relay", SiteSpec::Known(0)),
                rec("c", 7000, "Twin", SiteSpec::At(own)),
                rec("d", 7100, "Nowhere", SiteSpec::Known(9999)),
            ],
        };
        let both = Catalog::build(&[&eibi.data, &mine], &eibi.vocabulary, &eibi.gazetteer);
        (catalog(), both)
    }

    #[test]
    fn sources_build_one_catalog() {
        let (alone, both) = two_sources();
        assert_eq!(both.len(), 13, "the record at an unknown site is left out");
        // EiBi's sites keep their ids; the two records of their own share one site after them
        assert_eq!(both.sites.len(), alone.sites.len() + 1);
        assert!(alone.sites.iter().all(|s| both.site(s.id) == Some(s)));
        let ids: Vec<u32> = both.stations().map(|s| s.id).collect();
        assert_eq!(ids, (0..13).collect::<Vec<_>>());
        assert!(both.stations().collect::<Vec<_>>().windows(2).all(|w| w[0].freq_hz <= w[1].freq_hz));

        let local = both.by_key("mine", "a").unwrap();
        assert_eq!(
            (local.freq_hz, local.name.as_str(), local.mode, local.source.as_str()),
            (6_075_000, "Radio Local", Mode::USB, "mine")
        );
        assert_eq!(both.site(local.site).unwrap().name, "Santiago");
        assert_eq!(both.by_key("mine", "c").unwrap().site, local.site);
        let cnr = alone.between(6_070_000, 6_070_000).next().unwrap();
        assert_eq!(both.by_key("eibi", &cnr.key).map(|s| s.name.as_str()), Some(cnr.name.as_str()));
        assert!(both.by_key("mine", "d").is_none() && both.by_key("eibi", "a").is_none());

        // both sources answer a lookup; the relay stands at EiBi's site 0
        let l = both.lookup(6_070_000, at(10, 0), SANTIAGO, LOOKUP_TOLERANCE_HZ);
        let found: Vec<(&str, &str)> = l.iter().map(|c| (c.station.source.as_str(), c.station.name.as_str())).collect();
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.contains(&("mine", "Radio Relay")) && found.contains(&("eibi", cnr.name.as_str())));
        assert_eq!(l.iter().find(|c| c.station.source == "mine").unwrap().site.id, 0);

        let m = both.meta(at(10, 0));
        let rows: Vec<(&str, u32, bool)> = m.sources.iter().map(|s| (s.id.as_str(), s.rows, s.current)).collect();
        assert_eq!(rows, [("eibi", 10, true), ("mine", 3, true)]);
        assert_eq!(m.langs.iter().find(|l| l.code == "S").unwrap().rows, 4);
        assert_eq!(both.stamps().iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["eibi", "mine"]);
        // the globe counts the listener's rows per site: two at Santiago, the relay at site 0
        let o = both.overview(at(10, 0));
        assert_eq!(o.sites.iter().map(|s| s.mine).sum::<u32>(), 3);
        assert_eq!((o.sites[0].mine, o.sites[local.site as usize].mine), (1, 2));
        assert!(alone.overview(at(10, 0)).sites.iter().all(|s| s.mine == 0));
    }

    #[test]
    fn search_filters_and_counts_sources() {
        let (_, both) = two_sources();
        let t = at(10, 0);
        let p = both.search(&query(|q| q.facets = true), t);
        let f = p.facets.unwrap();
        assert_eq!(f.sources, BTreeMap::from([("eibi".to_string(), 10), ("mine".to_string(), 3)]));
        let p = both.search(
            &query(|q| {
                q.sources = vec!["mine".into()];
                q.facets = true;
            }),
            t,
        );
        assert_eq!(names(&p.items), [(6070, "Radio Relay"), (6075, "Radio Local"), (7000, "Twin")]);
        assert!(p.items.iter().all(|r| r.source == "mine"));
        let f = p.facets.unwrap();
        // the source facet leaves its own filter out; the others count Mine's rows
        assert_eq!(f.sources.get("eibi"), Some(&10));
        assert_eq!(f.countries.values().sum::<u32>(), 3);
        assert_eq!(both.search(&query(|q| q.q = "radio local".into()), t).total, 1);
        assert_eq!(names(&both.search(&query(|q| q.q = "ferry".into()), t).items), [(6075, "Radio Local")]);
    }

    #[test]
    fn code_tables_offer_every_code_and_the_known_sites() {
        let s = parse(&files()).unwrap().into_source();
        let c = Catalog::build(&[&s.data], &s.vocabulary, &s.gazetteer);
        let t = c.code_tables(&s.vocabulary, &s.gazetteer);
        assert_eq!(t.countries.len(), COUNTRIES.len());
        assert_eq!(t.countries.iter().find(|c| c.itu == "KOR").map(|c| c.rows), Some(3));
        // the built-in languages and the README's, with the catalog's counts
        let lang = |code: &str| t.langs.iter().find(|l| l.code == code);
        assert_eq!(lang("S").map(|l| l.rows), Some(1));
        assert_eq!(lang("AB").map(|l| (l.en.as_str(), l.rows)), Some(("Abkhaz", 0)));
        assert!(t.langs.len() >= LANGS.len());
        // targets without the countries, which the country list offers
        assert!(t.targets.iter().any(|t| t.code == "CAm"));
        assert!(t.targets.iter().all(|t| country(&t.code).is_none()));
        // precise sites only, by country and name
        assert!(t.sites.iter().all(|s| s.precise));
        assert!(t.sites.windows(2).all(|w| (&w[0].itu, &w[0].name) <= (&w[1].itu, &w[1].name)));
        let goyang = t.sites.iter().find(|s| s.code.as_deref() == Some("g")).unwrap();
        assert_eq!(c.site(goyang.id), Some(goyang));
        assert!(t.placeable.iter().any(|c| c == "BUL") && t.placeable.iter().any(|c| c == "CHN"));
        assert!(!t.placeable.iter().any(|c| c == "CHL"));
    }

    #[test]
    fn a_cell_replaces_the_catalog_as_the_next_generation() {
        let cell = CatalogCell::new(catalog());
        let before = cell.get();
        assert_eq!((before.generation(), before.overview(0).generation, before.meta(0).generation), (0, 0, 0));
        cell.replace(two_sources().1);
        let after = cell.get();
        assert_eq!((after.generation(), after.len()), (1, 13));
        assert_eq!(after.meta(0).generation, 1);
        // a snapshot taken before keeps its catalog
        assert_eq!((before.generation(), before.len()), (0, 10));
    }
}
