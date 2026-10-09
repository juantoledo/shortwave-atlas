//! EiBi shortwave schedules (http://www.eibispace.de/dx/): `sked-<season>.csv` plus the
//! lookup tables in README.TXT, turned into broadcast entries with resolved transmitter
//! sites. Pure: the caller reads the files.
//!
//! Rows are kept or skipped in this order: invalid, out of band (1711-30000 kHz),
//! inactive (P=8), utility (P>=90, a utility language code, or a name in the utility
//! patterns), not a broadcast (Days = alt/harm/imod/spur/LSB/USB), no site.

pub mod fields;
pub mod names;
pub mod readme;

use std::collections::{BTreeMap, HashMap, HashSet};

use regex::{RegexSet, RegexSetBuilder};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::calendar::Season;
use crate::country::country;
use crate::schedule::{Days, SeasonOnly, Slot};
use crate::text::decode_eibi;
use fields::{
    parse_days, parse_ddmm, parse_khz, parse_last_heard, parse_persistence, parse_site_code, parse_time, DaysField,
};
pub use readme::{parse_readme, Readme};

/// Broadcast frequencies kept, in kHz (shortwave, from the 120 m tropical band up).
pub const MIN_KHZ: u32 = 1711;
pub const MAX_KHZ: u32 = 30_000;
/// `--strict` limit: more invalid rows than this fraction fails the import.
pub const MAX_INVALID_FRACTION: f64 = 0.02;

/// The raw files, as read from disk or bundled.
pub struct EibiFiles<'a> {
    pub csv: &'a [u8],
    /// File name of the CSV (`sked-a26.csv`), for the season.
    pub csv_name: &'a str,
    pub readme: &'a [u8],
    /// `site_overrides.csv`: `itu,code,lat,lon,name,note`.
    pub overrides: &'a str,
    /// `utility_patterns.txt`: one case-insensitive regex per line.
    pub utility: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EibiError {
    MissingColumns(Vec<String>),
    BadPattern(String),
    BadOverride { line: usize, reason: String },
}

impl std::fmt::Display for EibiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingColumns(c) => write!(f, "EiBi CSV is missing columns: {}", c.join(", ")),
            Self::BadPattern(e) => write!(f, "bad utility pattern: {e}"),
            Self::BadOverride { line, reason } => write!(f, "site_overrides.csv line {line}: {reason}"),
        }
    }
}

impl std::error::Error for EibiError {}

/// A transmitter site with coordinates. `precise = false`: the row's site is unknown, so it
/// stands at a known site of the country (named after the country).
#[derive(Clone, Debug, PartialEq)]
pub struct SiteRec {
    pub itu: String,
    pub code: Option<String>,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub precise: bool,
}

/// One kept EiBi row.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub freq_hz: u32,
    /// Index into `Parsed::stations`.
    pub station: u32,
    /// Index into `Parsed::sites`.
    pub site: u32,
    pub slot: Slot,
}

/// Rows skipped, by reason.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Skipped {
    pub invalid: u32,
    pub band: u32,
    pub inactive: u32,
    pub utility: u32,
    pub not_broadcast: u32,
    pub no_site: u32,
}

/// What the import did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ParseReport {
    pub source_file: String,
    /// `A26`, if the file name says.
    pub season: Option<String>,
    pub total_rows: u32,
    pub kept: u32,
    pub skipped: Skipped,
    pub sites: u32,
    pub precise_sites: u32,
    pub stations: u32,
    pub day_patterns: u32,
    /// Unknown Days values (kept as irregular), unresolved sites and the like.
    pub warnings: Vec<String>,
}

impl ParseReport {
    /// Fail an import with too many invalid rows, or nothing kept.
    pub fn check_strict(&self) -> Result<(), String> {
        if self.kept == 0 {
            return Err(format!("{}: no broadcasts kept", self.source_file));
        }
        let bad = f64::from(self.skipped.invalid) / f64::from(self.total_rows.max(1));
        if bad > MAX_INVALID_FRACTION {
            return Err(format!(
                "{}: {} of {} rows invalid ({:.1}%)",
                self.source_file,
                self.skipped.invalid,
                self.total_rows,
                bad * 100.0
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Parsed {
    pub season: Option<Season>,
    pub readme: Readme,
    pub sites: Vec<SiteRec>,
    /// `(name, itu)`
    pub stations: Vec<(String, String)>,
    pub entries: Vec<Entry>,
    pub report: ParseReport,
}

/// The CSV columns we read, by header name (EiBi appends `:width` to each).
const COLUMNS: [(&str, &[&str]); 11] = [
    ("freq", &["khz"]),
    ("time", &["time(utc)", "time"]),
    ("days", &["days"]),
    ("itu", &["itu"]),
    ("station", &["station"]),
    ("lang", &["lng", "lang", "language"]),
    ("target", &["target"]),
    ("site", &["remarks"]),
    ("persistence", &["p"]),
    ("start", &["start"]),
    ("stop", &["stop"]),
];
const REQUIRED: [&str; 6] = ["freq", "time", "itu", "station", "site", "persistence"];

/// Column index per field name.
fn map_header(header: &str) -> Result<HashMap<&'static str, usize>, EibiError> {
    let cells: Vec<String> =
        header.split(';').map(|c| c.split(':').next().unwrap_or("").trim().to_ascii_lowercase()).collect();
    let mut map = HashMap::new();
    for (field, aliases) in COLUMNS {
        if let Some(i) = cells.iter().position(|c| aliases.contains(&c.as_str())) {
            map.insert(field, i);
        }
    }
    let missing: Vec<String> = REQUIRED.iter().filter(|f| !map.contains_key(*f)).map(|f| f.to_string()).collect();
    if missing.is_empty() {
        Ok(map)
    } else {
        Err(EibiError::MissingColumns(missing))
    }
}

/// `(itu, code)` -> `(name, lat, lon)`; code empty = the country's default site.
type Overrides = HashMap<(String, String), (String, f64, f64)>;
/// A site's identity: `(itu, code, name, lat bits, lon bits, precise)`.
type SiteKey = (String, Option<String>, String, u64, u64, bool);

/// `site_overrides.csv`.
fn parse_overrides(text: &str) -> Result<Overrides, EibiError> {
    let mut out = HashMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || (i == 0 && line.starts_with("itu,")) {
            continue;
        }
        let bad = |reason: &str| EibiError::BadOverride { line: i + 1, reason: reason.into() };
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() < 5 {
            return Err(bad("expected itu,code,lat,lon,name[,note]"));
        }
        let lat: f64 = f[2].parse().map_err(|_| bad("bad lat"))?;
        let lon: f64 = f[3].parse().map_err(|_| bad("bad lon"))?;
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(bad("lat/lon out of range"));
        }
        out.insert((f[0].to_string(), f[1].to_string()), (f[4].to_string(), lat, lon));
    }
    Ok(out)
}

fn utility_set(text: &str) -> Result<RegexSet, EibiError> {
    let pats: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    RegexSetBuilder::new(&pats).case_insensitive(true).build().map_err(|e| EibiError::BadPattern(e.to_string()))
}

/// A language code that marks a utility transmission (`-CW`, `-TS`...); `-MX` is music.
fn utility_lang(lang: &str) -> bool {
    lang.starts_with('-') && lang != "-MX"
}

/// Site resolution, see the module docs: override, the README's site, the country's default
/// site, any known site of the country (imprecise), or none.
struct SiteResolver<'a> {
    readme: &'a Readme,
    overrides: Overrides,
    sites: Vec<SiteRec>,
    index: HashMap<SiteKey, u32>,
}

impl SiteResolver<'_> {
    fn add(&mut self, s: SiteRec) -> u32 {
        let key = (s.itu.clone(), s.code.clone(), s.name.clone(), s.lat.to_bits(), s.lon.to_bits(), s.precise);
        *self.index.entry(key).or_insert_with(|| {
            self.sites.push(s);
            (self.sites.len() - 1) as u32
        })
    }

    fn resolve(&mut self, itu: &str, code: Option<&str>) -> Option<u32> {
        let code_key = (itu.to_string(), code.unwrap_or("").to_string());
        let rec = |name: &str, (lat, lon): (f64, f64), precise: bool| SiteRec {
            itu: itu.to_string(),
            code: if precise { code.map(str::to_string) } else { None },
            name: name.to_string(),
            lat,
            lon,
            precise,
        };
        if let Some((name, lat, lon)) = self.overrides.get(&code_key).cloned() {
            return Some(self.add(rec(&name, (lat, lon), true)));
        }
        if let Some(site) = self.readme.sites.get(&code_key) {
            if let Some(pos) = site.pos {
                let name = site.name.clone();
                return Some(self.add(rec(&name, pos, true)));
            }
        }
        let &pos = self.readme.first_site.get(itu)?;
        let name = country(itu)
            .map(|c| c.en.to_string())
            .or_else(|| self.readme.countries.get(itu).cloned())
            .unwrap_or_else(|| itu.to_string());
        Some(self.add(rec(&name, pos, false)))
    }
}

/// Parse the EiBi files.
pub fn parse(files: &EibiFiles) -> Result<Parsed, EibiError> {
    let readme = parse_readme(&decode_eibi(files.readme));
    let csv = decode_eibi(files.csv);
    let utility = utility_set(files.utility)?;
    let season = Season::from_filename(files.csv_name);
    let mut lines = csv.lines();
    let header = lines.next().unwrap_or("");
    let cols = map_header(header)?;

    let mut report = ParseReport {
        source_file: files.csv_name.rsplit(['/', '\\']).next().unwrap_or(files.csv_name).to_string(),
        season: season.map(|s| s.code()),
        ..Default::default()
    };
    let mut resolver = SiteResolver {
        readme: &readme,
        overrides: parse_overrides(files.overrides)?,
        sites: Vec::new(),
        index: HashMap::new(),
    };
    let mut stations: Vec<(String, String)> = Vec::new();
    let mut station_ix: HashMap<(String, String), u32> = HashMap::new();
    let mut entries = Vec::new();
    let mut unknown_days: BTreeMap<String, u32> = BTreeMap::new();
    let mut unresolved: BTreeMap<String, u32> = BTreeMap::new();

    for line in lines.filter(|l| !l.trim().is_empty()) {
        report.total_rows += 1;
        let cells: Vec<&str> = line.split(';').map(str::trim).collect();
        let get = |f: &str| cols.get(f).and_then(|&i| cells.get(i)).copied().unwrap_or("");

        let (Some(freq_hz), Some((start, end)), Some(p)) =
            (parse_khz(get("freq")), parse_time(get("time")), parse_persistence(get("persistence")))
        else {
            report.skipped.invalid += 1;
            continue;
        };
        if get("itu").is_empty() || get("station").is_empty() {
            report.skipped.invalid += 1;
            continue;
        }
        if !(MIN_KHZ * 1000..=MAX_KHZ * 1000).contains(&freq_hz) {
            report.skipped.band += 1;
            continue;
        }
        if p == 8 {
            report.skipped.inactive += 1;
            continue;
        }
        if p >= 90 || utility_lang(get("lang")) || utility.is_match(get("station")) {
            report.skipped.utility += 1;
            continue;
        }
        let days = match parse_days(get("days")) {
            DaysField::NotBroadcast => {
                report.skipped.not_broadcast += 1;
                continue;
            }
            DaysField::Days(d) => d,
            DaysField::Unknown(s) => {
                *unknown_days.entry(s).or_default() += 1;
                Days::Irregular
            }
        };
        let itu = get("itu");
        let (site_itu, code) = parse_site_code(get("site"), itu);
        let Some(site) = resolver.resolve(site_itu, code) else {
            report.skipped.no_site += 1;
            *unresolved.entry(format!("{site_itu}-{}", code.unwrap_or(""))).or_default() += 1;
            continue;
        };
        let name = get("station").to_string();
        let station = *station_ix.entry((name.clone(), itu.to_string())).or_insert_with(|| {
            stations.push((name, itu.to_string()));
            (stations.len() - 1) as u32
        });
        let date = |f: &str| season.zip(parse_ddmm(get(f))).and_then(|(s, (d, m))| s.ddmm(d, m));
        let (from, to) = if p == 6 { (date("start"), date("stop")) } else { (None, None) };
        let slot = Slot {
            start,
            end,
            days,
            lang: get("lang").to_string(),
            target: get("target").to_string(),
            season: match p {
                4 => SeasonOnly::Winter,
                5 => SeasonOnly::Summer,
                _ => SeasonOnly::All,
            },
            from,
            to,
            heard: parse_last_heard(get("stop")).or_else(|| parse_last_heard(get("start"))),
        };
        entries.push(Entry { freq_hz, station, site, slot });
    }

    let sites = resolver.sites;
    report.kept = entries.len() as u32;
    report.sites = sites.len() as u32;
    report.precise_sites = sites.iter().filter(|s| s.precise).count() as u32;
    report.stations = stations.len() as u32;
    report.day_patterns = entries.iter().map(|e| e.slot.days).collect::<HashSet<_>>().len() as u32;
    report.warnings.extend(unknown_days.iter().map(|(d, n)| format!("unknown days {d:?} ({n}x), kept as irregular")));
    report.warnings.extend(unresolved.iter().map(|(s, n)| format!("no site for {s} ({n}x)")));
    Ok(Parsed { season, readme, sites, stations, entries, report })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::calendar::{days_from_civil, iso_date};

    pub use super::readme::tests::README;

    pub const UTILITY: &str = "# comment\nvolmet\n\\bspy numbers\\b\n";
    pub const OVERRIDES: &str = "itu,code,lat,lon,name,note\nKOR,x,37.5,127.0,Seoul test site,for the tests\n";

    /// One row per rule, with the real header.
    pub const CSV: &str = "\
kHz:75;Time(UTC):93;Days:59;ITU:49;Station:201;Lng:49;Target:62;Remarks:135;P:35;Start:60;Stop:60;
5900;1700-1800;247;KOR;New Korea Hope Bcing;K;KRE;g;1;0106;
5910;1430-1500;;BUL;Radio Bulgaria;E;WEu;p;0;;
5915;1600-1700;;CHN;China Radio Int.;HI;SAs;ka;0;;
6070;0000-2400;;CHN;China Radio Int.;M;CHN;;1;;
7350;2300-0100;Mo;CLA;Radio Free Test;S;CAm;/BUL-s;6;2903;1104[0426]
9410;0500-0600;;G;BBC;E;Eu;/AFS-j;5;;
9411;0500-0600;;G;BBC;E;Eu;/AFS;4;;[0923]
11000;0000-0100;;KOR;KBS World Radio;K;FE;x;1;;
11010;0000-0100;Test;KOR;KBS World Radio;K;FE;k;1;;
1386;1830-1900;;G;BBC;R;EEu;/LTU-v;6;2903;3006[0725]
5900;2000-2058;;KOR;Unification Media Group;K;KRE;g;8;;
8828;0000-2400;;HKG;Hongkong Volmet x15,x45;E;FE;;1;;
5000;0000-2400;;CHN;BPM time signal;-TS;FE;;1;;
5800;0000-0100;;CHN;E06 Russian Spy Numbers;E;Eu;;98;;
9000;0000-0100;alt;CHN;China Radio Int.;M;FE;ka;1;;
9001;0000-0100;;CLA;Radio Nowhere;E;FE;;1;;
9002;0000-0100;;USA;Mystery;E;NAm;xx;1;;
abc;0000-0100;;USA;Bad freq;E;NAm;;1;;
9003;25:00-0100;;USA;Bad time;E;NAm;;1;;
";

    pub fn files() -> EibiFiles<'static> {
        EibiFiles {
            csv: CSV.as_bytes(),
            csv_name: "sked-a26.csv",
            readme: README.as_bytes(),
            overrides: OVERRIDES,
            utility: UTILITY,
        }
    }

    pub fn parsed() -> Parsed {
        parse(&files()).expect("fixture parses")
    }

    #[test]
    fn rows_are_filtered_in_order() {
        let r = parsed().report;
        assert_eq!(r.total_rows, 19);
        assert_eq!(
            r.skipped,
            Skipped { invalid: 2, band: 1, inactive: 1, utility: 3, not_broadcast: 1, no_site: 1 },
            "{r:#?}"
        );
        assert_eq!(r.kept, 10);
        assert_eq!(r.season.as_deref(), Some("A26"));
        assert_eq!(r.source_file, "sked-a26.csv");
        assert!(r.warnings.iter().any(|w| w.contains("\"Test\"")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("no site for CLA-")), "{:?}", r.warnings);
        // 2 invalid rows of 19 is over the 2% limit
        assert!(r.check_strict().is_err());
    }

    fn entry(p: &Parsed, khz: u32) -> (&Entry, &SiteRec, &(String, String)) {
        let e = p.entries.iter().find(|e| e.freq_hz == khz * 1000).unwrap_or_else(|| panic!("{khz} kHz kept"));
        (e, &p.sites[e.site as usize], &p.stations[e.station as usize])
    }

    #[test]
    fn sites_resolve_by_override_code_default_and_country() {
        let p = parsed();
        let (_, s, st) = entry(&p, 5900);
        assert_eq!(
            (s.itu.as_str(), s.code.as_deref(), s.name.as_str(), s.precise),
            ("KOR", Some("g"), "Goyang / Gyeonggi-do", true)
        );
        assert_eq!(st, &("New Korea Hope Bcing".to_string(), "KOR".to_string()));
        // relay abroad, with a code
        let (_, s, _) = entry(&p, 7350);
        assert_eq!((s.itu.as_str(), s.name.as_str()), ("BUL", "Sofia-Kostinbrod"));
        // relay abroad, the country's default site
        let (_, s, _) = entry(&p, 9411);
        assert_eq!((s.name.as_str(), s.lat, s.precise), ("Meyerton", -26.5833, true));
        // no code and no default: any site of the country, named after the country
        let (_, s, _) = entry(&p, 6070);
        assert_eq!((s.name.as_str(), s.code.as_deref(), s.precise, s.lat), ("China", None, false, 39.35));
        // an override
        let (_, s, _) = entry(&p, 11000);
        assert_eq!((s.name.as_str(), s.lat, s.precise), ("Seoul test site", 37.5, true));
        // a code without coordinates: the country fallback
        let (_, s, _) = entry(&p, 9002);
        assert!(!s.precise);
        assert_eq!(s.name, "United States");
    }

    #[test]
    fn sites_are_shared() {
        let p = parsed();
        let china: Vec<_> = p.sites.iter().filter(|s| s.itu == "CHN").collect();
        assert_eq!(china.len(), 2, "{china:?}"); // ka (twice) and the country fallback
        assert_eq!(p.report.sites as usize, p.sites.len());
        assert_eq!(p.report.precise_sites as usize, p.sites.iter().filter(|s| s.precise).count());
        assert_eq!(p.report.stations, 7);
    }

    #[test]
    fn slots_carry_days_season_and_dates() {
        let p = parsed();
        let (e, _, _) = entry(&p, 7350);
        assert_eq!((e.slot.start, e.slot.end), (1380, 60));
        assert_eq!(e.slot.days, Days::Weekly { mask: 1 });
        assert_eq!(e.slot.from.map(iso_date), Some("2026-03-29".into()));
        assert_eq!(e.slot.to.map(iso_date), Some("2026-04-11".into()));
        assert_eq!(e.slot.heard.as_deref(), Some("2026-04"));
        let (e, _, _) = entry(&p, 9410);
        assert_eq!(e.slot.season, SeasonOnly::Summer);
        let (e, _, _) = entry(&p, 9411);
        assert_eq!((e.slot.season, e.slot.heard.as_deref()), (SeasonOnly::Winter, Some("2023-09")));
        // dates only for P=6
        let (e, _, _) = entry(&p, 5900);
        assert_eq!((e.slot.from, e.slot.days), (None, Days::Weekly { mask: 0b100_1010 }));
        let (e, _, _) = entry(&p, 11010);
        assert_eq!(e.slot.days, Days::Irregular);
        assert_eq!(e.slot.lang, "K");
        assert_eq!(e.slot.target, "FE");
    }

    #[test]
    fn combined_languages_are_kept_whole() {
        let csv = CSV.replace("5910;1430-1500;;BUL;Radio Bulgaria;E;", "5910;1430-1500;;BUL;Radio Bulgaria;S,Q;");
        let f = EibiFiles { csv: csv.as_bytes(), ..files() };
        let p = parse(&f).unwrap();
        assert_eq!(p.entries.iter().find(|e| e.freq_hz == 5_910_000).unwrap().slot.lang, "S,Q");
    }

    #[test]
    fn b_season_dates_cross_the_new_year() {
        let csv = CSV.replace("2903;1104[0426]", "0111;0401");
        let f = EibiFiles { csv: csv.as_bytes(), csv_name: "sked-b26.csv", ..files() };
        let p = parse(&f).unwrap();
        let e = p.entries.iter().find(|e| e.freq_hz == 7_350_000).unwrap();
        assert_eq!(e.slot.from, Some(days_from_civil(2026, 11, 1)));
        assert_eq!(e.slot.to, Some(days_from_civil(2027, 1, 4)));
    }

    #[test]
    fn latin1_input_and_reordered_columns() {
        let csv = b"Station;kHz;Time(UTC);ITU;Remarks;P\nR\xe1dio Nacional;9410;0000-0100;G;/AFS;1\n";
        let f = EibiFiles { csv, ..files() };
        let p = parse(&f).unwrap();
        assert_eq!(p.stations[0].0, "Rádio Nacional");
        assert_eq!(p.entries[0].slot.days, Days::Daily);
    }

    #[test]
    fn missing_columns_are_an_error() {
        let f = EibiFiles { csv: b"kHz;Time(UTC);Station\n", ..files() };
        assert_eq!(
            parse(&f).unwrap_err(),
            EibiError::MissingColumns(vec!["itu".into(), "site".into(), "persistence".into()])
        );
    }

    #[test]
    fn bad_patterns_and_overrides_are_errors() {
        assert!(matches!(parse(&EibiFiles { utility: "(", ..files() }), Err(EibiError::BadPattern(_))));
        let o = "itu,code,lat,lon,name,note\nKOR,x,95,0,Nowhere,\n";
        assert!(matches!(parse(&EibiFiles { overrides: o, ..files() }), Err(EibiError::BadOverride { line: 2, .. })));
    }

    #[test]
    fn strict_check() {
        let mut r = ParseReport { source_file: "x.csv".into(), total_rows: 100, kept: 90, ..Default::default() };
        r.skipped.invalid = 2;
        assert!(r.check_strict().is_ok());
        r.skipped.invalid = 3;
        assert!(r.check_strict().is_err());
        r.skipped.invalid = 0;
        r.kept = 0;
        assert!(r.check_strict().is_err());
    }

    #[test]
    fn deterministic() {
        let (a, b) = (parsed(), parsed());
        assert_eq!(a.entries, b.entries);
        assert_eq!(a.sites, b.sites);
    }
}

/// The bundled data against radiomap's published counts (`data/eibi/parity.json`).
#[cfg(test)]
mod parity {
    use std::path::Path;

    use super::*;

    #[derive(Deserialize)]
    struct Expected {
        source_file: String,
        rows: u32,
        skipped: Skipped,
        sites: u32,
        precise_sites: u32,
        stations: u32,
        day_patterns: u32,
        atlas_keeps: Vec<String>,
    }

    #[test]
    fn bundled_data_matches_radiomap() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/eibi");
        let read = |p: &str| std::fs::read(dir.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
        let exp: Expected = serde_json::from_slice(&read("parity.json")).expect("parity.json");
        let csv = read(&format!("source/{}", exp.source_file));
        let readme = read("source/README.TXT");
        let overrides = String::from_utf8(read("site_overrides.csv")).unwrap();
        let utility = String::from_utf8(read("utility_patterns.txt")).unwrap();
        let files = EibiFiles {
            csv: &csv,
            csv_name: &exp.source_file,
            readme: &readme,
            overrides: &overrides,
            utility: &utility,
        };

        let ours = parse(&files).expect("bundled EiBi parses");
        ours.report.check_strict().expect("bundled EiBi passes the strict check");

        // radiomap also drops `atlas_keeps` as utilities: do the same for the comparison
        let mut as_radiomap = utility.clone();
        for name in &exp.atlas_keeps {
            as_radiomap.push_str(&format!("\n^{}$", regex::escape(name)));
        }
        let r = parse(&EibiFiles { utility: &as_radiomap, ..files }).unwrap().report;
        let got = (r.kept, &r.skipped, r.sites, r.precise_sites, r.stations, r.day_patterns);
        let want = (exp.rows, &exp.skipped, exp.sites, exp.precise_sites, exp.stations, exp.day_patterns);
        assert_eq!(got, want, "(kept, skipped, sites, precise sites, stations, day patterns) differ from radiomap");
        let kept_extra = ours.report.kept - r.kept;
        assert!(
            exp.atlas_keeps.is_empty() || kept_extra > 0,
            "atlas_keeps no longer in the data: {:?}",
            exp.atlas_keeps
        );
    }
}
