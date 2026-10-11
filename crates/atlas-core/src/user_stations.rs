//! The listener's own stations: the "Mine" source. Each is a station on one frequency from one
//! transmitter, with an EiBi-like schedule, its id being its `Record::key` (so favorites and the
//! logbook follow it through edits). Kept in `user-stations.json`, which is also what export
//! writes and import reads. Pure: the server reads and writes the file and rebuilds the catalog.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::country::country;
use crate::eibi::names::{lang_name, target};
use crate::eibi::SiteRec;
use crate::rig::Mode;
use crate::scan::valid_id;
use crate::schedule::{Days, SeasonOnly, Slot};
use crate::source::{Gazetteer, Record, SiteSpec, SourceData, SourceDesc, SourceKind, Vocabulary};
use crate::stations::Catalog;
use crate::text::fold;

/// `SourceDesc::id` of the listener's stations.
pub const MINE: &str = "mine";
/// `UserStationsFile::version` of what this build writes.
pub const FORMAT_VERSION: u32 = 1;
pub const MAX_STATIONS: usize = 500;
pub const MAX_SLOTS: usize = 24;
/// Languages per slot.
pub const MAX_LANGS: usize = 4;
/// Longest texts, in characters.
pub const MAX_NAME: usize = 80;
pub const MAX_SITE_NAME: usize = 60;
pub const MAX_NOTES: usize = 2000;
pub const MAX_URL: usize = 300;
/// The frequencies a station may be on, kHz: from longwave up through UHF.
pub const MIN_KHZ: u32 = 10;
pub const MAX_KHZ: u32 = 2_000_000;

/// Where a station's transmitter stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum UserSite {
    /// One of the known sites (`CodeTables::sites`), by what identifies it, since site ids change.
    /// Where it stood is kept for when the data no longer has it.
    Known { itu: String, code: Option<String>, name: String, lat: f64, lon: f64 },
    /// Where the listener says.
    Custom { itu: String, name: String, lat: f64, lon: f64 },
    /// Somewhere in a country: shown at a known site of it, as EiBi shows unknown sites.
    Country { itu: String },
}

/// One time on the air.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserSlot {
    /// UTC minutes; `start > end` crosses midnight, an `end` of 1440 is midnight.
    pub start: u16,
    pub end: u16,
    /// `Daily` or `Weekly`.
    pub days: Days,
    /// Language codes, `S,Q`.
    pub lang: String,
    /// A target area code, or empty.
    pub target: String,
}

impl UserSlot {
    fn to_slot(&self) -> Slot {
        Slot {
            start: self.start,
            end: self.end,
            days: self.days,
            lang: self.lang.clone(),
            target: self.target.clone(),
            season: SeasonOnly::All,
            from: None,
            to: None,
            heard: None,
        }
    }
}

/// A station's fields: what `Call::AddUserStation` and `Call::UpdateUserStation` take.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserStationDraft {
    pub freq_hz: u32,
    pub mode: Mode,
    pub name: String,
    /// The station's own country.
    pub itu: String,
    pub site: UserSite,
    pub slots: Vec<UserSlot>,
    pub notes: String,
    /// Its website or stream.
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserStation {
    /// `1791648000-5905000`: when it was added and the frequency.
    pub id: String,
    #[serde(flatten)]
    pub station: UserStationDraft,
    /// Unix seconds.
    #[ts(type = "number")]
    pub added: i64,
    #[ts(type = "number")]
    pub updated: i64,
}

/// `user-stations.json`, and an export.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserStationsFile {
    pub version: u32,
    /// Unix seconds, in an export.
    #[serde(default)]
    pub exported: Option<i64>,
    pub stations: Vec<UserStation>,
}

/// A station and its row in the running catalog (result of `Call::UserStations`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserStationView {
    pub station: UserStation,
    /// `None`: its site cannot be placed.
    pub station_id: Option<u32>,
    /// Its known site is no longer in the data: it is shown where the site stood.
    pub site_missing: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ImportSkip {
    pub name: String,
    pub reason: String,
}

/// What an import did (result of `Call::ImportUserStations`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ImportReport {
    pub added: u32,
    /// Stations already kept that the file has a newer copy of.
    pub updated: u32,
    pub unchanged: u32,
    pub skipped: Vec<ImportSkip>,
}

fn site_rec(itu: &str, code: Option<String>, name: &str, lat: f64, lon: f64, precise: bool) -> SiteRec {
    SiteRec { itu: itu.to_string(), code, name: name.to_string(), lat, lon, precise }
}

/// Where `site` stands among the gazetteer's sites; `None` if it cannot be placed (a country
/// without a known site). The flag: a known site the gazetteer no longer has, placed where it
/// stood.
pub fn place(site: &UserSite, gaz: &Gazetteer) -> Option<(SiteSpec, bool)> {
    match site {
        UserSite::Known { itu, code, name, lat, lon } => {
            let found = gaz.sites.iter().position(|s| {
                s.precise
                    && &s.itu == itu
                    && match code {
                        Some(c) => s.code.as_ref() == Some(c),
                        None => s.code.is_none() && &s.name == name,
                    }
            });
            Some(match found {
                Some(i) => (SiteSpec::Known(i as u32), false),
                None => (SiteSpec::At(site_rec(itu, code.clone(), name, *lat, *lon, true)), true),
            })
        }
        UserSite::Custom { itu, name, lat, lon } => {
            Some((SiteSpec::At(site_rec(itu, None, name, *lat, *lon, true)), false))
        }
        UserSite::Country { itu } => {
            if let Some(i) = gaz.sites.iter().position(|s| &s.itu == itu && !s.precise) {
                return Some((SiteSpec::Known(i as u32), false));
            }
            let &(lat, lon) = gaz.first_site.get(itu)?;
            let name = country(itu).map_or_else(|| itu.clone(), |c| c.en.to_string());
            Some((SiteSpec::At(site_rec(itu, None, &name, lat, lon, false)), false))
        }
    }
}

/// Trimmed, without control characters (but `keep`).
fn clean(s: &str, keep: &[char]) -> String {
    s.trim().chars().filter(|c| !c.is_control() || keep.contains(c)).collect()
}

fn too_long(s: &str, max: usize) -> bool {
    s.chars().count() > max
}

fn known_country(itu: &str) -> Result<(), String> {
    match country(itu) {
        Some(_) => Ok(()),
        None => Err(format!("Unknown country code {itu:?}")),
    }
}

fn check_pos(lat: f64, lon: f64) -> Result<(), String> {
    if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
        Ok(())
    } else {
        Err("The site's position is out of range".into())
    }
}

fn check_slot(s: &UserSlot, vocab: &Vocabulary) -> Result<UserSlot, String> {
    if s.start >= 1440 || s.end > 1440 || s.start == s.end {
        return Err("A time on the air starts and ends at two different times of the day".into());
    }
    match s.days {
        Days::Daily => {}
        Days::Weekly { mask } if (1..=127).contains(&mask) => {}
        Days::Weekly { .. } => return Err("Pick at least one day".into()),
        _ => return Err("A time on the air is daily or on some days of the week".into()),
    }
    let langs: Vec<&str> = s.lang.split(',').map(str::trim).filter(|l| !l.is_empty()).collect();
    if langs.len() > MAX_LANGS {
        return Err(format!("A time on the air has at most {MAX_LANGS} languages"));
    }
    if let Some(l) = langs.iter().find(|l| lang_name(l).is_none() && !vocab.langs.contains_key(**l)) {
        return Err(format!("Unknown language code {l:?}"));
    }
    let t = s.target.trim();
    if !(t.is_empty() || target(t).is_some() || country(t).is_some() || vocab.targets.contains_key(t)) {
        return Err(format!("Unknown target area {t:?}"));
    }
    Ok(UserSlot { lang: langs.join(","), target: t.to_string(), ..s.clone() })
}

fn check_site(site: &UserSite) -> Result<UserSite, String> {
    let name = |n: &str| {
        let n = clean(n, &[]);
        match () {
            _ if n.is_empty() => Err("The site needs a name".to_string()),
            _ if too_long(&n, MAX_SITE_NAME) => Err(format!("A site's name has at most {MAX_SITE_NAME} characters")),
            _ => Ok(n),
        }
    };
    Ok(match site {
        UserSite::Known { itu, code, name: n, lat, lon } => {
            known_country(itu)?;
            check_pos(*lat, *lon)?;
            UserSite::Known { itu: itu.clone(), code: code.clone(), name: name(n)?, lat: *lat, lon: *lon }
        }
        UserSite::Custom { itu, name: n, lat, lon } => {
            known_country(itu)?;
            check_pos(*lat, *lon)?;
            UserSite::Custom { itu: itu.clone(), name: name(n)?, lat: *lat, lon: *lon }
        }
        UserSite::Country { itu } => {
            known_country(itu)?;
            site.clone()
        }
    })
}

/// `d`, trimmed, if it makes a station; else why not.
pub fn check(d: &UserStationDraft, vocab: &Vocabulary, gaz: &Gazetteer) -> Result<UserStationDraft, String> {
    if !(MIN_KHZ * 1000..=MAX_KHZ * 1000).contains(&d.freq_hz) {
        return Err(format!("The frequency must be between {MIN_KHZ} kHz and {} MHz", MAX_KHZ / 1000));
    }
    let name = clean(&d.name, &[]);
    if name.is_empty() {
        return Err("A station needs a name".into());
    }
    if too_long(&name, MAX_NAME) {
        return Err(format!("A name has at most {MAX_NAME} characters"));
    }
    let itu = d.itu.trim().to_string();
    known_country(&itu)?;
    if d.slots.is_empty() {
        return Err("A station needs at least one time on the air".into());
    }
    if d.slots.len() > MAX_SLOTS {
        return Err(format!("A station has at most {MAX_SLOTS} times on the air"));
    }
    let slots = d.slots.iter().map(|s| check_slot(s, vocab)).collect::<Result<Vec<_>, _>>()?;
    let site = check_site(&d.site)?;
    if place(&site, gaz).is_none() {
        return Err("There is no known site in that country to show the station at: give the site's position".into());
    }
    let notes = clean(&d.notes, &['\n']);
    if too_long(&notes, MAX_NOTES) {
        return Err(format!("Notes have at most {MAX_NOTES} characters"));
    }
    let url = match d.url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        None => None,
        Some(u) if too_long(u, MAX_URL) => return Err(format!("A link has at most {MAX_URL} characters")),
        Some(u) => {
            let lower = u.to_ascii_lowercase();
            if !(lower.starts_with("http://") || lower.starts_with("https://")) || u.contains(char::is_whitespace) {
                return Err("A link starts with http:// or https://".into());
            }
            Some(u.to_string())
        }
    };
    Ok(UserStationDraft { freq_hz: d.freq_hz, mode: d.mode, name, itu, site, slots, notes, url })
}

/// A station's id: when it was added and the frequency, made unique among `list`.
fn new_id(list: &[UserStation], added: i64, freq_hz: u32) -> String {
    let base = format!("{added}-{freq_hz}");
    (1..)
        .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
        .find(|id| !list.iter().any(|u| &u.id == id))
        .expect("some suffix is free")
}

/// Keep a new station.
pub fn add(
    list: &mut Vec<UserStation>,
    d: &UserStationDraft,
    vocab: &Vocabulary,
    gaz: &Gazetteer,
    now: i64,
) -> Result<UserStation, String> {
    let station = check(d, vocab, gaz)?;
    if list.len() >= MAX_STATIONS {
        return Err(format!("There are already {MAX_STATIONS} stations"));
    }
    let u = UserStation { id: new_id(list, now, station.freq_hz), station, added: now, updated: now };
    list.push(u.clone());
    Ok(u)
}

/// Change station `id` (it keeps its id); `None` if there is none.
pub fn update(
    list: &mut [UserStation],
    id: &str,
    d: &UserStationDraft,
    vocab: &Vocabulary,
    gaz: &Gazetteer,
    now: i64,
) -> Result<Option<UserStation>, String> {
    let station = check(d, vocab, gaz)?;
    Ok(list.iter_mut().find(|u| u.id == id).map(|u| {
        u.station = station;
        u.updated = now;
        u.clone()
    }))
}

/// Forget station `id`; `false` if there is none.
pub fn remove(list: &mut Vec<UserStation>, id: &str) -> bool {
    let before = list.len();
    list.retain(|u| u.id != id);
    list.len() != before
}

/// The stations as a data source. Those whose site cannot be placed are left out.
pub fn to_source(list: &[UserStation], gaz: &Gazetteer) -> SourceData {
    let records = list
        .iter()
        .filter_map(|u| {
            let (site, _) = place(&u.station.site, gaz)?;
            let s = &u.station;
            Some(Record {
                key: u.id.clone(),
                freq_hz: s.freq_hz,
                mode: s.mode,
                name: s.name.clone(),
                itu: s.itu.clone(),
                site,
                slots: s.slots.iter().map(UserSlot::to_slot).collect(),
                notes: s.notes.clone(),
            })
        })
        .collect();
    let desc = SourceDesc {
        id: MINE.into(),
        name: "Mine".into(),
        kind: SourceKind::User,
        editable: true,
        url: None,
        // the last change
        version: list.iter().map(|u| u.updated).max().unwrap_or(0).to_string(),
        season: None,
        report: None,
    };
    SourceData { desc, records }
}

/// Every station with its row in `cat`, by frequency.
pub fn views(list: &[UserStation], cat: &Catalog, gaz: &Gazetteer) -> Vec<UserStationView> {
    let mut out: Vec<UserStationView> = list
        .iter()
        .map(|u| UserStationView {
            station: u.clone(),
            station_id: cat.by_key(MINE, &u.id).map(|st| st.id),
            site_missing: place(&u.station.site, gaz).is_some_and(|(_, missing)| missing),
        })
        .collect();
    out.sort_by(|a, b| {
        a.station.station.freq_hz.cmp(&b.station.station.freq_hz).then_with(|| a.station.added.cmp(&b.station.added))
    });
    out
}

/// `list` as an export.
pub fn export(list: &[UserStation], now: i64) -> String {
    let file = UserStationsFile { version: FORMAT_VERSION, exported: Some(now), stations: list.to_vec() };
    serde_json::to_string_pretty(&file).expect("stations serialise")
}

/// Read an export (or a `user-stations.json`).
pub fn parse_file(json: &str) -> Result<UserStationsFile, String> {
    let file: UserStationsFile =
        serde_json::from_str(json).map_err(|e| format!("This is not a file of stations: {e}"))?;
    if file.version > FORMAT_VERSION {
        return Err("These stations are from a newer Shortwave Atlas: update it to import them".into());
    }
    Ok(file)
}

/// Bring `file`'s stations in. `replace`: instead of those kept. Otherwise a station already
/// kept (the same id, or the same frequency, name and country) takes the file's copy when it is
/// newer. Stations that do not check out, or do not fit, are skipped and said why.
pub fn import(
    list: &mut Vec<UserStation>,
    file: &UserStationsFile,
    replace: bool,
    vocab: &Vocabulary,
    gaz: &Gazetteer,
    now: i64,
) -> ImportReport {
    if replace {
        list.clear();
    }
    let same = |a: &UserStationDraft, b: &UserStationDraft| {
        a.freq_hz == b.freq_hz && a.itu == b.itu && fold(&a.name) == fold(&b.name)
    };
    let mut r = ImportReport::default();
    for u in &file.stations {
        let skip = |reason: String| ImportSkip { name: u.station.name.clone(), reason };
        let station = match check(&u.station, vocab, gaz) {
            Ok(s) => s,
            Err(e) => {
                r.skipped.push(skip(e));
                continue;
            }
        };
        let at =
            list.iter().position(|x| x.id == u.id).or_else(|| list.iter().position(|x| same(&x.station, &station)));
        match at {
            Some(i) if u.updated > list[i].updated => {
                list[i].station = station;
                list[i].updated = u.updated;
                r.updated += 1;
            }
            Some(_) => r.unchanged += 1,
            None if list.len() >= MAX_STATIONS => {
                r.skipped.push(skip(format!("There are already {MAX_STATIONS} stations")))
            }
            None => {
                let id = if valid_id(&u.id) && !list.iter().any(|x| x.id == u.id) {
                    u.id.clone()
                } else {
                    new_id(list, now, station.freq_hz)
                };
                list.push(UserStation { id, station, added: u.added, updated: u.updated });
                r.added += 1;
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eibi::{parse, tests::files, EibiSource};

    fn eibi() -> EibiSource {
        parse(&files()).unwrap().into_source()
    }

    fn slot() -> UserSlot {
        UserSlot {
            start: 1380,
            end: 60,
            days: Days::Weekly { mask: 0b10 },
            lang: " S, Q ".into(),
            target: "SAm".into(),
        }
    }

    fn draft(khz: u32, name: &str, site: UserSite) -> UserStationDraft {
        UserStationDraft {
            freq_hz: khz * 1000,
            mode: Mode::USB,
            name: name.into(),
            itu: "CHL".into(),
            site,
            slots: vec![slot()],
            notes: "  heard on the ferry\n".into(),
            url: Some(" https://radio.example/live ".into()),
        }
    }

    fn custom() -> UserSite {
        UserSite::Custom { itu: "CHL".into(), name: " Santiago ".into(), lat: -33.45, lon: -70.67 }
    }

    #[test]
    fn drafts_are_trimmed_and_checked() {
        let s = eibi();
        let (v, g) = (&s.vocabulary, &s.gazetteer);
        let d = check(&draft(6075, "  Radio Local\u{7} ", custom()), v, g).unwrap();
        assert_eq!(
            (d.name.as_str(), d.notes.as_str(), d.url.as_deref()),
            ("Radio Local", "heard on the ferry", Some("https://radio.example/live"))
        );
        assert_eq!((d.slots[0].lang.as_str(), d.slots[0].target.as_str()), ("S,Q", "SAm"));
        assert!(matches!(&d.site, UserSite::Custom { name, .. } if name == "Santiago"));

        let bad = |f: fn(&mut UserStationDraft)| {
            let mut d = draft(6075, "Radio Local", custom());
            f(&mut d);
            check(&d, v, g).unwrap_err()
        };
        assert!(bad(|d| d.freq_hz = 5_000).contains("frequency"));
        assert!(bad(|d| d.name = "   ".into()).contains("name"));
        assert!(bad(|d| d.name = "x".repeat(MAX_NAME + 1)).contains("80"));
        assert!(bad(|d| d.itu = "XYZ".into()).contains("XYZ"));
        assert!(bad(|d| d.slots.clear()).contains("at least one"));
        assert!(bad(|d| d.slots = vec![slot(); MAX_SLOTS + 1]).contains("24"));
        assert!(bad(|d| d.slots[0].end = d.slots[0].start).contains("different"));
        assert!(bad(|d| d.slots[0].start = 1440).contains("different"));
        assert!(bad(|d| d.slots[0].days = Days::Weekly { mask: 0 }).contains("day"));
        assert!(bad(|d| d.slots[0].days = Days::Irregular).contains("week"));
        assert!(bad(|d| d.slots[0].lang = "S,ZZZ".into()).contains("ZZZ"));
        assert!(bad(|d| d.slots[0].lang = "S,Q,E,K,M".into()).contains("4 languages"));
        assert!(bad(|d| d.slots[0].target = "Nowhere".into()).contains("Nowhere"));
        assert!(bad(|d| d.site = UserSite::Custom { itu: "CHL".into(), name: "x".into(), lat: 91.0, lon: 0.0 })
            .contains("range"));
        assert!(bad(|d| d.site = UserSite::Custom { itu: "CHL".into(), name: " ".into(), lat: 0.0, lon: 0.0 })
            .contains("site needs a name"));
        // Chile has no site in the fixture to stand a station at
        assert!(bad(|d| d.site = UserSite::Country { itu: "CHL".into() }).contains("no known site"));
        assert!(bad(|d| d.url = Some("javascript:alert(1)".into())).contains("http"));
        assert!(bad(|d| d.notes = "é".repeat(MAX_NOTES + 1)).contains("2000"));
        // no target, no language, no link: fine
        let mut d = draft(6075, "Radio Local", custom());
        d.slots[0] = UserSlot { lang: String::new(), target: String::new(), days: Days::Daily, ..slot() };
        d.url = Some("  ".into());
        assert_eq!(check(&d, v, g).unwrap().url, None);
        // a country is a target too, and the README's languages count
        d.slots[0].target = "CHL".into();
        d.slots[0].lang = "AB".into();
        assert!(check(&d, v, g).is_ok());
    }

    #[test]
    fn sites_are_placed_among_the_known_ones() {
        let s = eibi();
        let g = &s.gazetteer;
        let goyang = g.sites.iter().position(|s| s.code.as_deref() == Some("g")).unwrap() as u32;
        let known = |code: Option<&str>, name: &str| UserSite::Known {
            itu: "KOR".into(),
            code: code.map(Into::into),
            name: name.into(),
            lat: 37.0,
            lon: 127.0,
        };
        assert_eq!(place(&known(Some("g"), "renamed"), g), Some((SiteSpec::Known(goyang), false)));
        // gone from the data: where it stood
        let Some((SiteSpec::At(rec), true)) = place(&known(Some("zz"), "Old site"), g) else { panic!() };
        assert_eq!((rec.name.as_str(), rec.lat, rec.precise), ("Old site", 37.0, true));
        // a country: its imprecise site if the data has one, else its first known site
        let china = g.sites.iter().position(|s| s.itu == "CHN" && !s.precise).unwrap() as u32;
        assert_eq!(place(&UserSite::Country { itu: "CHN".into() }, g), Some((SiteSpec::Known(china), false)));
        let Some((SiteSpec::At(bul), false)) = place(&UserSite::Country { itu: "BUL".into() }, g) else { panic!() };
        assert_eq!((bul.name.as_str(), bul.precise), ("Bulgaria", false));
        assert_eq!(place(&UserSite::Country { itu: "CHL".into() }, g), None);
    }

    #[test]
    fn add_update_remove() {
        let s = eibi();
        let (v, g) = (&s.vocabulary, &s.gazetteer);
        let mut list = vec![];
        let a = add(&mut list, &draft(6075, "Radio Local", custom()), v, g, 100).unwrap();
        assert_eq!((a.id.as_str(), a.added, a.updated), ("100-6075000", 100, 100));
        let b = add(&mut list, &draft(6075, "Twin", custom()), v, g, 100).unwrap();
        assert_eq!(b.id, "100-6075000-2");
        let moved = update(&mut list, &a.id, &draft(6080, "Radio Local FM", custom()), v, g, 200).unwrap().unwrap();
        assert_eq!(
            (moved.id.as_str(), moved.station.freq_hz, moved.added, moved.updated),
            (a.id.as_str(), 6_080_000, 100, 200)
        );
        assert_eq!(update(&mut list, "nope", &draft(6080, "x", custom()), v, g, 200), Ok(None));
        assert!(update(&mut list, &a.id, &draft(6080, "", custom()), v, g, 200).is_err());
        assert!(remove(&mut list, &b.id) && !remove(&mut list, &b.id));
        assert_eq!(list.len(), 1);
        for i in 1..MAX_STATIONS as u32 {
            add(&mut list, &draft(7000 + i, "Many", custom()), v, g, 0).unwrap();
        }
        assert!(add(&mut list, &draft(6075, "One more", custom()), v, g, 0).unwrap_err().contains("500"));
    }

    #[test]
    fn as_a_source_in_the_catalog() {
        let s = eibi();
        let (v, g) = (&s.vocabulary, &s.gazetteer);
        let mut list = vec![];
        let a = add(&mut list, &draft(6075, "Radio Local", custom()), v, g, 100).unwrap();
        let k = add(
            &mut list,
            &draft(
                5900,
                "At Goyang",
                UserSite::Known {
                    itu: "KOR".into(),
                    code: Some("g".into()),
                    name: "Goyang".into(),
                    lat: 37.0,
                    lon: 127.0,
                },
            ),
            v,
            g,
            100,
        )
        .unwrap();
        // a country that lost its site since: kept, but without a row
        let gone = UserStation {
            id: "1-1".into(),
            station: UserStationDraft { site: UserSite::Country { itu: "CHL".into() }, ..a.station.clone() },
            added: 1,
            updated: 1,
        };
        list.push(gone);
        let mine = to_source(&list, g);
        assert_eq!((mine.desc.id.as_str(), mine.desc.version.as_str(), mine.records.len()), ("mine", "100", 2));
        let cat = Catalog::build(&[&s.data, &mine], v, g);
        let st = cat.by_key(MINE, &a.id).unwrap();
        assert_eq!((st.mode, st.slots[0].start, st.slots[0].lang.as_str()), (Mode::USB, 1380, "S,Q"));
        let v = views(&list, &cat, g);
        assert_eq!(v.iter().map(|v| v.station.id.as_str()).collect::<Vec<_>>(), [k.id.as_str(), "1-1", a.id.as_str()]);
        assert_eq!(v[0].station_id, cat.by_key(MINE, &k.id).map(|s| s.id));
        assert_eq!(cat.site(cat.by_key(MINE, &k.id).unwrap().site).unwrap().code.as_deref(), Some("g"));
        assert_eq!((v[1].station_id, v[0].site_missing), (None, false));
    }

    #[test]
    fn export_and_import() {
        let s = eibi();
        let (v, g) = (&s.vocabulary, &s.gazetteer);
        let mut here = vec![];
        let a = add(&mut here, &draft(6075, "Radio Local", custom()), v, g, 100).unwrap();
        let mut there = here.clone();
        update(&mut there, &a.id, &draft(6075, "Radio Local", UserSite::Country { itu: "BUL".into() }), v, g, 300)
            .unwrap();
        add(&mut there, &draft(7000, "New one", custom()), v, g, 300).unwrap();
        let mut broken = there[1].clone();
        broken.id = "x".into();
        broken.station.name = String::new();
        there.push(broken);

        let file = parse_file(&export(&there, 400)).unwrap();
        assert_eq!((file.version, file.exported), (FORMAT_VERSION, Some(400)));
        let r = import(&mut here, &file, false, v, g, 500);
        assert_eq!((r.added, r.updated, r.unchanged), (1, 1, 0));
        assert_eq!(r.skipped.len(), 1);
        assert!(r.skipped[0].reason.contains("name"), "{:?}", r.skipped);
        assert_eq!(here.len(), 2);
        assert!(matches!(here[0].station.site, UserSite::Country { .. }), "the newer copy won");
        // again: nothing new
        assert_eq!(
            import(&mut here, &file, false, v, g, 500),
            ImportReport { unchanged: 2, skipped: r.skipped.clone(), ..Default::default() }
        );
        // matched by frequency, name and country when the ids differ; an older copy loses
        let mut old = file.clone();
        old.stations.truncate(1);
        old.stations[0].id = "9-9".into();
        old.stations[0].station.name = "RADIO LOCAL".into();
        old.stations[0].updated = 1;
        assert_eq!(import(&mut here, &old, false, v, g, 500).unchanged, 1);
        // replace: only the file's
        let r = import(&mut here, &old, true, v, g, 500);
        assert_eq!((r.added, here.len(), here[0].id.as_str()), (1, 1, "9-9"));
        // an id that is no good as one is replaced
        old.stations[0].id = "../x".into();
        import(&mut here, &old, true, v, g, 500);
        assert_eq!(here[0].id, "500-6075000");

        assert!(parse_file("{").unwrap_err().contains("not a file"));
        let newer = serde_json::to_string(&UserStationsFile { version: FORMAT_VERSION + 1, ..file }).unwrap();
        assert!(parse_file(&newer).unwrap_err().contains("newer"));
    }

    #[test]
    fn json() {
        let s = eibi();
        let mut list = vec![];
        add(&mut list, &draft(6075, "Radio Local", custom()), &s.vocabulary, &s.gazetteer, 1).unwrap();
        let j = serde_json::to_value(&list[0]).unwrap();
        assert_eq!(
            (j["id"].as_str(), j["freq_hz"].as_u64(), j["site"]["kind"].as_str()),
            (Some("1-6075000"), Some(6_075_000), Some("custom"))
        );
        assert_eq!(j["slots"][0]["days"]["t"], "weekly");
        let back: UserStation = serde_json::from_value(j).unwrap();
        assert_eq!(back, list[0]);
    }
}
