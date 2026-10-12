//! The game layer of the listening history, all from the sessions: the passport (countries
//! heard, by region), rarity and points, badges and the monthly bingo card. Calm by design:
//! these are facts about the history, worked out again on every ask; nothing is stored.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::calendar::{civil_from_days, days_from_civil};
use crate::country::{country, Region, COUNTRIES};
use crate::geo::SunPhase;
use crate::listening::Session;
use crate::schedule::{Days, Slot, DAY_MIN};
use crate::stats::{day_start, local_day};

/// Half the Earth's circumference: the antipode.
pub const ANTIPODE_KM: f32 = 20_015.0;
/// S3 in dB over S9 (6 dB per S unit).
pub const FAINT_DB: i16 = -36;

/// Minutes a week a station is on the air, roughly (seasons and dates aside).
pub fn weekly_minutes(slots: &[Slot]) -> f64 {
    slots
        .iter()
        .map(|s| {
            let (a, b) = (u32::from(s.start), u32::from(s.end));
            let mins = if a < b { b - a } else if a > b { DAY_MIN - a + b } else { 0 };
            let days = match s.days {
                Days::Daily => 7.0,
                Days::Weekly { mask } => f64::from((mask & 0x7f).count_ones()),
                Days::Nth { .. } | Days::Last { .. } => 0.25,
                Days::Date { .. } => 7.0 / 365.0,
                // irregular: assume once a week
                Days::Irregular => 1.0,
            };
            f64::from(mins) * days
        })
        .sum::<f64>()
        .min(f64::from(7 * DAY_MIN))
}

/// 1 (six hours a day or more) to 5 (under an hour a week).
pub fn rarity(slots: &[Slot]) -> u8 {
    match weekly_minutes(slots) {
        m if m >= 2520.0 => 1,
        m if m >= 840.0 => 2,
        m if m >= 210.0 => 3,
        m if m >= 60.0 => 4,
        _ => 5,
    }
}

/// The five continents (the Middle East counts as Asia).
pub fn continent(r: Region) -> Option<Region> {
    match r {
        Region::Me => Some(Region::As),
        Region::Other => None,
        r => Some(r),
    }
}

const CONTINENTS: [Region; 5] = [Region::Am, Region::Eu, Region::Af, Region::As, Region::Oc];
const REGIONS: [Region; 6] = [Region::Am, Region::Eu, Region::Af, Region::Me, Region::As, Region::Oc];

/// A country in the passport.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Stamp {
    pub itu: String,
    /// flag-icons key.
    pub flag: Option<String>,
    pub region: Region,
    /// First heard (Unix seconds).
    #[ts(type = "number")]
    pub first: i64,
    pub seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RegionDone {
    pub region: Region,
    pub heard: u32,
    pub total: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BadgeId {
    /// An hour in all between midnight and 05:00 local.
    NightOwl,
    /// An hour in all between 05:00 and 08:00 local.
    EarlyBird,
    /// Two hours in one session.
    Marathon,
    /// Five bands in one day.
    BandHopper,
    /// The same station seven days running.
    Loyal,
    /// Twenty languages.
    Babel,
    /// A station within 2,000 km of the antipode.
    Antipode,
    /// Ten sessions with either end in twilight.
    GreyLine,
    /// Two minutes or more on a station at S3 or less.
    FaintDx,
    /// Every continent.
    AllContinents,
    /// A station on a tropical band (120, 90, 75 or 60 m) more than 3,000 km away.
    TropicalDx,
    /// A station on 13 m or 11 m, the top of the shortwave dial.
    TopOfDial,
    /// A 5★ station: on the air less than an hour a week.
    RareCatch,
    /// A path all in the dark, more than 10,000 km long.
    LongDark,
    /// Dawn at the transmitter while it is night where you are.
    SunriseCatch,
    /// Listening in AM, SSB and CW.
    ModeExplorer,
}

/// The tropical bands, and the top of the dial (`eibi::names::BANDS` ids).
const TROPICAL: [&str; 4] = ["120m", "90m", "75m", "60m"];
const TOP_BANDS: [&str; 2] = ["13m", "11m"];

/// The kind of mode a session was in: AM, SSB or CW (the rest count for none).
fn mode_kind(mode: Option<&str>) -> Option<u8> {
    match mode? {
        "AM" | "AMN" | "SAM" => Some(0),
        "USB" | "LSB" => Some(1),
        "CW" | "CWR" => Some(2),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Badge {
    pub id: BadgeId,
    /// When it was earned.
    #[ts(type = "number | null")]
    pub earned: Option<i64>,
    /// Towards `goal`, in the badge's unit (minutes, bands, days, ...).
    pub progress: u32,
    pub goal: u32,
}

/// One square of the bingo card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Challenge {
    /// A station from this region.
    Region { region: Region },
    /// A broadcast in this EiBi language.
    Lang { lang: String },
    Band { band: String },
    BelowKhz { khz: u32 },
    AboveKhz { khz: u32 },
    OverKm { km: u32 },
    UnderKm { km: u32 },
    /// A station this rare or rarer.
    Rarity { stars: u8 },
    GreyLine,
    /// The whole path in the dark.
    DarkPath,
    /// One session this long.
    Minutes { minutes: u32 },
    /// A country never heard before this month.
    NewCountry,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Cell {
    pub challenge: Challenge,
    #[ts(type = "number | null")]
    pub done: Option<i64>,
}

/// This month's card: 3×3, row by row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Bingo {
    pub year: i32,
    pub month: u32,
    pub cells: Vec<Cell>,
    /// Rows, columns and diagonals complete.
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Collection {
    /// By first heard.
    pub passport: Vec<Stamp>,
    pub regions: Vec<RegionDone>,
    /// The continents heard, of the five.
    pub continents: Vec<Region>,
    /// This month: each station's rarity once.
    pub points: u32,
    pub badges: Vec<Badge>,
    pub bingo: Bingo,
}

fn pool() -> Vec<Challenge> {
    use Challenge as C;
    let mut v: Vec<Challenge> = REGIONS.iter().map(|&region| C::Region { region }).collect();
    v.extend(["A", "S", "F", "R", "P", "E", "M"].iter().map(|l| C::Lang { lang: l.to_string() }));
    v.extend(["49m", "41m", "31m", "25m", "19m"].iter().map(|b| C::Band { band: b.to_string() }));
    v.extend([
        C::BelowKhz { khz: 3_000 },
        C::AboveKhz { khz: 15_000 },
        C::OverKm { km: 10_000 },
        C::UnderKm { km: 1_000 },
        C::Rarity { stars: 4 },
        C::GreyLine,
        C::DarkPath,
        C::Minutes { minutes: 30 },
        C::NewCountry,
    ]);
    v
}

/// The card for a month: the same on every client (seeded by the month).
pub fn card(year: i32, month: u32) -> Vec<Challenge> {
    let mut pool = pool();
    let mut seed = (year as u64 * 12 + u64::from(month)).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = vec![];
    while out.len() < 9 && !pool.is_empty() {
        // xorshift64
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let i = (seed % pool.len() as u64) as usize;
        let c = pool.swap_remove(i);
        // one region and one language at most, for variety
        let kind = |c: &Challenge| std::mem::discriminant(c);
        let limited = matches!(c, Challenge::Region { .. } | Challenge::Lang { .. } | Challenge::Band { .. });
        if limited && out.iter().any(|o| kind(o) == kind(&c)) {
            continue;
        }
        out.push(c);
    }
    out
}

/// The EiBi languages of the session's slot (`S,Q` is two).
pub fn langs_of(s: &Session) -> Vec<&str> {
    s.station.as_ref().map_or(vec![], |r| r.lang.split(',').map(str::trim).filter(|l| !l.is_empty()).collect())
}

fn region_of(s: &Session) -> Option<Region> {
    s.country().and_then(country).map(|c| c.region)
}

/// Whether `s` meets challenge `c`; `before` holds the countries heard before the month.
fn meets(c: &Challenge, s: &Session, before: &HashSet<String>) -> bool {
    let id = s.station.is_some();
    match c {
        Challenge::Region { region } => region_of(s) == Some(*region),
        Challenge::Lang { lang } => langs_of(s).contains(&lang.as_str()),
        Challenge::Band { band } => id && &s.band == band,
        Challenge::BelowKhz { khz } => id && s.freq_hz < khz * 1000,
        Challenge::AboveKhz { khz } => id && s.freq_hz > khz * 1000,
        Challenge::OverKm { km } => s.km.is_some_and(|k| k > *km as f32),
        Challenge::UnderKm { km } => s.km.is_some_and(|k| k < *km as f32),
        Challenge::Rarity { stars } => s.rarity.is_some_and(|r| r >= *stars),
        Challenge::GreyLine => id && s.sky.is_some_and(|k| k.grey_line()),
        Challenge::DarkPath => id && s.sky.is_some_and(|k| k.dark >= 1.0),
        Challenge::Minutes { minutes } => s.secs() >= i64::from(*minutes) * 60,
        Challenge::NewCountry => s.country().is_some_and(|c| !before.contains(c)),
    }
}

/// Seconds of `[start, end)` between `from_h` and `to_h` local, on any day.
fn hours_overlap(start: i64, end: i64, tz_min: i32, from_h: i64, to_h: i64) -> i64 {
    let mut total = 0;
    for day in local_day(start, tz_min)..=local_day(end, tz_min) {
        let d0 = day_start(day, tz_min);
        let (a, b) = (d0 + from_h * 3600, d0 + to_h * 3600);
        total += (end.min(b) - start.max(a)).max(0);
    }
    total
}

/// The game layer for `all` (every session, oldest first, the open one last) at `now`.
pub fn collection(all: &[&Session], now: i64, tz_min: i32) -> Collection {
    let today = local_day(now, tz_min);
    let (year, month, _) = civil_from_days(today);
    let month_start = day_start(days_from_civil(year, month, 1), tz_min);

    // passport
    let mut stamps: HashMap<&str, Stamp> = HashMap::new();
    for s in all {
        if let Some(c) = s.country().and_then(country) {
            let st = stamps.entry(c.itu).or_insert(Stamp { itu: c.itu.into(), flag: c.flag.map(String::from), region: c.region, first: s.start, seconds: 0 });
            st.seconds = st.seconds.saturating_add(s.secs() as u32);
        }
    }
    let mut passport: Vec<Stamp> = stamps.into_values().collect();
    passport.sort_by(|a, b| a.first.cmp(&b.first).then_with(|| a.itu.cmp(&b.itu)));
    let regions = REGIONS
        .iter()
        .map(|&r| RegionDone {
            region: r,
            heard: passport.iter().filter(|s| s.region == r).count() as u32,
            total: COUNTRIES.iter().filter(|c| c.region == r).count() as u32,
        })
        .collect();
    let heard_cont: HashSet<Region> = passport.iter().filter_map(|s| continent(s.region)).collect();
    let continents = CONTINENTS.iter().copied().filter(|c| heard_cont.contains(c)).collect();

    // points: each station's rarity once this month
    let mut seen = HashSet::new();
    let points = all
        .iter()
        .filter(|s| s.end > month_start)
        .filter_map(|s| Some((s.station.as_ref()?.ident(), s.rarity?)))
        .filter(|(k, _)| seen.insert(k.clone()))
        .map(|(_, r)| u32::from(r))
        .sum();

    // bingo
    let before: HashSet<String> =
        all.iter().filter(|s| s.start < month_start).filter_map(|s| s.country().map(String::from)).collect();
    let cells: Vec<Cell> = card(year, month)
        .into_iter()
        .map(|challenge| {
            let done = all.iter().filter(|s| s.start >= month_start && meets(&challenge, s, &before)).map(|s| s.end).min();
            Cell { challenge, done }
        })
        .collect();
    const LINES: [[usize; 3]; 8] = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6]];
    let lines = if cells.len() == 9 {
        LINES.iter().filter(|l| l.iter().all(|&i| cells[i].done.is_some())).count() as u32
    } else {
        0
    };
    let bingo = Bingo { year, month, cells, lines };

    Collection { passport, regions, continents, points, badges: badges(all, tz_min), bingo }
}

/// Every badge, earned or on its way.
pub fn badges(all: &[&Session], tz_min: i32) -> Vec<Badge> {
    use BadgeId::*;
    let goals: [(BadgeId, u32); 16] = [
        (NightOwl, 60),
        (EarlyBird, 60),
        (Marathon, 120),
        (BandHopper, 5),
        (Loyal, 7),
        (Babel, 20),
        (Antipode, 1),
        (GreyLine, 10),
        (FaintDx, 1),
        (AllContinents, 5),
        (TropicalDx, 1),
        (TopOfDial, 1),
        (RareCatch, 1),
        (LongDark, 1),
        (SunriseCatch, 1),
        (ModeExplorer, 3),
    ];
    let mut b: Vec<Badge> = goals.iter().map(|&(id, goal)| Badge { id, earned: None, progress: 0, goal }).collect();
    let (mut night, mut early, mut grey) = (0i64, 0i64, 0u32);
    let mut day_bands: HashMap<i32, HashSet<&str>> = HashMap::new();
    let mut runs: HashMap<String, (i32, u32)> = HashMap::new();
    let mut langs_heard: HashSet<&str> = HashSet::new();
    let mut conts: HashSet<Region> = HashSet::new();
    let mut modes: HashSet<u8> = HashSet::new();
    for s in all {
        let mut up = |id: BadgeId, value: u32| {
            let badge = b.iter_mut().find(|x| x.id == id).expect("every badge");
            badge.progress = badge.progress.max(value.min(badge.goal));
            if badge.earned.is_none() && value >= badge.goal {
                badge.earned = Some(s.end);
            }
        };
        night += hours_overlap(s.start, s.end, tz_min, 0, 5);
        early += hours_overlap(s.start, s.end, tz_min, 5, 8);
        up(NightOwl, (night / 60) as u32);
        up(EarlyBird, (early / 60) as u32);
        up(Marathon, (s.secs() / 60) as u32);
        // modes count with or without a station (a CW net has none in the schedule)
        if let Some(k) = mode_kind(s.mode.as_deref()) {
            modes.insert(k);
        }
        up(ModeExplorer, modes.len() as u32);
        let day = local_day(s.start, tz_min);
        let Some(r) = &s.station else { continue };
        if s.band != crate::eibi::names::OUT_OF_BAND {
            let set = day_bands.entry(day).or_default();
            set.insert(&s.band);
            up(BandHopper, set.len() as u32);
        }
        let run = runs.entry(r.ident()).or_insert((day, 0));
        *run = match day - run.0 {
            0 if run.1 > 0 => *run,
            1 => (day, run.1 + 1),
            _ => (day, 1),
        };
        up(Loyal, run.1);
        langs_heard.extend(langs_of(s));
        up(Babel, langs_heard.len() as u32);
        up(Antipode, u32::from(s.km.is_some_and(|k| k >= ANTIPODE_KM - 2_000.0)));
        if s.sky.is_some_and(|k| k.grey_line()) {
            grey += 1;
        }
        up(GreyLine, grey);
        up(FaintDx, u32::from(s.secs() >= 120 && s.avg_db.is_some_and(|d| d <= FAINT_DB)));
        if let Some(c) = region_of(s).and_then(continent) {
            conts.insert(c);
        }
        up(AllContinents, conts.len() as u32);
        let km = s.km.unwrap_or(0.0);
        up(TropicalDx, u32::from(TROPICAL.contains(&s.band.as_str()) && km > 3_000.0));
        up(TopOfDial, u32::from(TOP_BANDS.contains(&s.band.as_str())));
        up(RareCatch, u32::from(s.rarity == Some(5)));
        up(LongDark, u32::from(s.sky.is_some_and(|k| k.dark >= 1.0) && km > 10_000.0));
        let sunrise = s.sky.is_some_and(|k| k.tx == SunPhase::Twilight && k.tx_morning && k.rx == SunPhase::Night);
        up(SunriseCatch, u32::from(sunrise));
    }
    b
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::geo::SunPhase;
    use crate::listening::tests::T0;
    use crate::listening::Sky;
    use crate::scan::StationRef;
    use crate::schedule::SeasonOnly;

    fn slot(start: u16, end: u16, days: Days) -> Slot {
        Slot {
            start,
            end,
            days,
            lang: "E".into(),
            target: String::new(),
            season: SeasonOnly::All,
            from: None,
            to: None,
            heard: None,
        }
    }

    fn refs(all: &[Session]) -> Vec<&Session> {
        all.iter().collect()
    }

    pub(crate) fn sref(name: &str, itu: &str, lang: &str) -> StationRef {
        StationRef {
            source: "eibi".into(),
            key: name.into(),
            freq_hz: 6_000_000,
            name: name.into(),
            itu: itu.into(),
            flag: None,
            site_name: String::new(),
            site_itu: itu.into(),
            lang: lang.into(),
            target: String::new(),
        }
    }

    /// A session of `mins` minutes from `start` on `station`.
    pub(crate) fn sess(start: i64, mins: i64, station: Option<StationRef>) -> Session {
        Session {
            id: format!("{start}"),
            start,
            end: start + mins * 60,
            freq_hz: 6_000_000,
            mode: Some("AM".into()),
            station,
            picked: false,
            km: Some(5_000.0),
            band: "49m".into(),
            best_db: None,
            avg_db: None,
            sky: None,
            rarity: Some(1),
        }
    }

    #[test]
    fn rarity_by_time_on_the_air() {
        assert_eq!(rarity(&[slot(0, 1440, Days::Daily)]), 1);
        assert_eq!(rarity(&[slot(1380, 60, Days::Daily)]), 2, "two hours a day, across midnight");
        assert_eq!(rarity(&[slot(600, 630, Days::Daily)]), 3);
        assert_eq!(rarity(&[slot(600, 700, Days::Weekly { mask: 1 })]), 4);
        assert_eq!(rarity(&[slot(600, 630, Days::Weekly { mask: 1 })]), 5);
        assert_eq!(rarity(&[slot(600, 900, Days::Date { d: 1, m: 1 })]), 5);
        assert_eq!(rarity(&[]), 5);
    }

    #[test]
    fn the_bingo_card_is_the_months() {
        let a = card(2026, 6);
        assert_eq!(a.len(), 9);
        assert_eq!(a, card(2026, 6), "the same card on every client");
        assert_ne!(a, card(2026, 7));
        // one region, one language and one band at most
        for c in a.iter().filter(|c| matches!(c, Challenge::Region { .. } | Challenge::Lang { .. } | Challenge::Band { .. })) {
            let same = a.iter().filter(|o| std::mem::discriminant(*o) == std::mem::discriminant(c)).count();
            assert_eq!(same, 1, "{a:?}");
        }
    }

    #[test]
    fn passport_regions_and_points() {
        let all = vec![
            sess(T0, 10, Some(sref("CRI", "CHN", "M"))),
            sess(T0 + 3600, 5, Some(sref("RNZ", "NZL", "E"))),
            sess(T0 + 7200, 5, Some(sref("CRI", "CHN", "M"))),
            sess(T0 + 9000, 5, None),
        ];
        let c = collection(&refs(&all), T0 + 10_000, 0);
        let p: Vec<(&str, u32)> = c.passport.iter().map(|s| (s.itu.as_str(), s.seconds)).collect();
        assert_eq!(p, [("CHN", 900), ("NZL", 300)]);
        let asia = c.regions.iter().find(|r| r.region == Region::As).unwrap();
        assert_eq!(asia.heard, 1);
        assert!(asia.total > 30);
        assert_eq!(c.continents, [Region::As, Region::Oc]);
        assert_eq!(c.points, 2, "each station once");
        assert_eq!(c.bingo.month, 6);
    }

    #[test]
    fn badges_at_their_thresholds() {
        let get = |all: &[Session], id| badges(&refs(all), 0).into_iter().find(|b| b.id == id).unwrap();
        let cri = || Some(sref("CRI", "CHN", "M"));
        // marathon: 119 min is not enough
        assert_eq!(get(&[sess(T0, 119, cri())], BadgeId::Marathon).earned, None);
        assert_eq!(get(&[sess(T0, 120, cri())], BadgeId::Marathon).earned, Some(T0 + 7200));
        // night owl: an hour between 00 and 05 local, over two nights; local time moves it
        let two = [sess(T0 + 4 * 3600, 30, cri()), sess(T0 + 86_400, 30, cri())];
        assert_eq!(get(&two, BadgeId::NightOwl).progress, 60);
        assert!(get(&two, BadgeId::NightOwl).earned.is_some());
        let b = badges(&refs(&two), -6 * 60).into_iter().find(|b| b.id == BadgeId::NightOwl).unwrap();
        assert_eq!(b.progress, 0, "18:00 and 22:00 at UTC-6");
        // loyal: 6 days running is not 7; a gap starts again
        let days = |n: i64, skip: Option<i64>| -> Vec<Session> {
            (0..n).filter(|d| Some(*d) != skip).map(|d| sess(T0 + d * 86_400 + 3600, 5, cri())).collect()
        };
        assert_eq!(get(&days(6, None), BadgeId::Loyal).progress, 6);
        assert!(get(&days(7, None), BadgeId::Loyal).earned.is_some());
        assert_eq!(get(&days(8, Some(3)), BadgeId::Loyal).progress, 4);
        // faint DX: S3 for two minutes, identified
        let faint = |mins, db, st| Session { avg_db: Some(db), ..sess(T0, mins, st) };
        assert!(get(&[faint(2, -36, cri())], BadgeId::FaintDx).earned.is_some());
        assert!(get(&[faint(2, -30, cri())], BadgeId::FaintDx).earned.is_none());
        assert!(get(&[faint(1, -40, cri())], BadgeId::FaintDx).earned.is_none());
        assert!(get(&[faint(5, -40, None)], BadgeId::FaintDx).earned.is_none());
        // grey line: ten catches
        let sky = Some(Sky { rx: SunPhase::Twilight, tx: SunPhase::Day, dark: 0.3, tx_morning: false });
        let grey: Vec<Session> = (0..10).map(|i| Session { sky, ..sess(T0 + i * 600, 5, cri()) }).collect();
        assert_eq!(get(&grey[..9], BadgeId::GreyLine).progress, 9);
        assert!(get(&grey, BadgeId::GreyLine).earned.is_some());
        // antipode, and every continent
        let far = Session { km: Some(18_100.0), ..sess(T0, 5, cri()) };
        assert!(get(&[far], BadgeId::Antipode).earned.is_some());
        let world: Vec<Session> = ["USA", "G", "NIG", "CHN", "AUS"]
            .iter()
            .enumerate()
            .map(|(i, c)| sess(T0 + i as i64 * 600, 5, Some(sref(c, c, "E"))))
            .collect();
        assert_eq!(get(&world[..4], BadgeId::AllContinents).progress, 4);
        assert!(get(&world, BadgeId::AllContinents).earned.is_some());

        // tropical DX: a tropical band, and far
        let on = |band: &str, km: f32| Session { band: band.into(), km: Some(km), ..sess(T0, 5, cri()) };
        assert!(get(&[on("60m", 3_100.0)], BadgeId::TropicalDx).earned.is_some());
        assert!(get(&[on("60m", 2_900.0)], BadgeId::TropicalDx).earned.is_none());
        assert!(get(&[on("49m", 9_000.0)], BadgeId::TropicalDx).earned.is_none());
        assert!(get(&[Session { station: None, ..on("90m", 5_000.0) }], BadgeId::TropicalDx).earned.is_none());
        // the top of the dial
        assert!(get(&[on("11m", 500.0)], BadgeId::TopOfDial).earned.is_some());
        assert!(get(&[on("16m", 500.0)], BadgeId::TopOfDial).earned.is_none());
        // a 5★ station
        assert!(get(&[Session { rarity: Some(5), ..sess(T0, 5, cri()) }], BadgeId::RareCatch).earned.is_some());
        assert!(get(&[Session { rarity: Some(4), ..sess(T0, 5, cri()) }], BadgeId::RareCatch).earned.is_none());
        // the long dark: all dark, and over 10,000 km
        let dark = |d: f32, km: f32| Session {
            sky: Some(Sky { rx: SunPhase::Night, tx: SunPhase::Night, dark: d, tx_morning: false }),
            ..on("31m", km)
        };
        assert!(get(&[dark(1.0, 10_500.0)], BadgeId::LongDark).earned.is_some());
        assert!(get(&[dark(0.9, 10_500.0)], BadgeId::LongDark).earned.is_none());
        assert!(get(&[dark(1.0, 9_500.0)], BadgeId::LongDark).earned.is_none());
        // sunrise there, night here; dusk there does not count
        let dawn = |morning: bool, rx| Session {
            sky: Some(Sky { rx, tx: SunPhase::Twilight, dark: 0.5, tx_morning: morning }),
            ..sess(T0, 5, cri())
        };
        assert!(get(&[dawn(true, SunPhase::Night)], BadgeId::SunriseCatch).earned.is_some());
        assert!(get(&[dawn(false, SunPhase::Night)], BadgeId::SunriseCatch).earned.is_none());
        assert!(get(&[dawn(true, SunPhase::Day)], BadgeId::SunriseCatch).earned.is_none());
        // AM, SSB and CW, the narrow and reversed variants included, a net without a station too
        let mode = |m: &str| Session { mode: Some(m.into()), station: None, ..sess(T0, 5, None) };
        assert_eq!(get(&[mode("AMN"), mode("LSB")], BadgeId::ModeExplorer).progress, 2);
        assert!(get(&[mode("AMN"), mode("LSB"), mode("CWR")], BadgeId::ModeExplorer).earned.is_some());
        assert_eq!(get(&[mode("FM"), mode("PKTUSB")], BadgeId::ModeExplorer).progress, 0);
    }

    #[test]
    fn bingo_cells_tick_from_this_months_sessions() {
        let card = card(2026, 6);
        let all = vec![
            Session {
                km: Some(12_000.0),
                rarity: Some(5),
                sky: Some(Sky { rx: SunPhase::Twilight, tx: SunPhase::Night, dark: 1.0, tx_morning: false }),
                ..sess(T0, 45, Some(sref("X", "AUS", "A,S,F,R,P,E,M")))
            },
            // last month's session counts for nothing, but makes NZL not new
            sess(T0 - 30 * 86_400, 600, Some(sref("Y", "NZL", "E"))),
        ];
        let c = collection(&refs(&all), T0 + 3600, 0);
        for cell in &c.bingo.cells {
            let expect = match &cell.challenge {
                Challenge::Region { region } => *region == Region::Oc,
                Challenge::Band { band } => band == "49m",
                Challenge::BelowKhz { .. } | Challenge::AboveKhz { .. } | Challenge::UnderKm { .. } => false,
                _ => true,
            };
            assert_eq!(cell.done.is_some(), expect, "{cell:?} in {card:?}");
        }
    }
}
