//! Single-field parsers for the EiBi CSV (`sked-*.csv`) and README.TXT.

use crate::schedule::Days;

const WEEKDAYS: [&str; 7] = ["mo", "tu", "we", "th", "fr", "sa", "su"];
const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// `HHMM-HHMM` to UTC minutes; `2400` is allowed as an end.
pub fn parse_time(s: &str) -> Option<(u16, u16)> {
    let (a, b) = s.trim().split_once('-')?;
    let hm = |t: &str, end: bool| -> Option<u16> {
        if t.len() != 4 || !t.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (h, m) = (t[..2].parse::<u16>().ok()?, t[2..].parse::<u16>().ok()?);
        let v = h * 60 + m;
        (m < 60 && (v < 1440 || (end && v == 1440))).then_some(v)
    };
    Some((hm(a, false)?, hm(b, true)?))
}

/// Frequency in kHz (`7277.5`) to Hz.
pub fn parse_khz(s: &str) -> Option<u32> {
    let khz: f64 = s.trim().parse().ok()?;
    (khz.is_finite() && khz > 0.0 && khz < 1e6).then(|| (khz * 1000.0).round() as u32)
}

/// Persistence code; empty means 0.
pub fn parse_persistence(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        Some(0)
    } else {
        s.parse().ok()
    }
}

/// A `DDMM` date at the start of the field (`0401` = 4 January), as `(day, month)`.
pub fn parse_ddmm(s: &str) -> Option<(u32, u32)> {
    let s = s.trim();
    let digits = s.get(..4).filter(|d| d.bytes().all(|b| b.is_ascii_digit()))?;
    let (d, m) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
    ((1..=31).contains(&d) && (1..=12).contains(&m)).then_some((d, m))
}

/// `[MMYY]` (last logged) anywhere in the field, as `YYYY-MM`.
pub fn parse_last_heard(s: &str) -> Option<String> {
    let i = s.find('[')?;
    let inner = s.get(i + 1..i + 5)?;
    if s.get(i + 5..i + 6) != Some("]") || !inner.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (m, y): (u32, u32) = (inner[..2].parse().ok()?, inner[2..].parse().ok()?);
    (1..=12).contains(&m).then(|| format!("{:04}-{m:02}", 2000 + y))
}

/// What the Days column says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaysField {
    Days(Days),
    /// Alternative frequency, harmonic, intermodulation, spurious, sideband: not a broadcast.
    NotBroadcast,
    /// Not understood: kept as irregular.
    Unknown(String),
}

fn weekday_token(s: &str) -> Option<u8> {
    WEEKDAYS.iter().position(|w| s.eq_ignore_ascii_case(w)).map(|i| i as u8)
}

/// `Mo-Fr`, `Tu,Fr`, `SaSu`, `We-Mo` (wraps) as a weekday mask.
fn weekday_list(s: &str) -> Option<u8> {
    let b = s.as_bytes();
    let (mut mask, mut i, mut prev, mut range) = (0u8, 0usize, None::<u8>, false);
    while i < b.len() {
        match b[i] {
            b',' => {
                range = false;
                i += 1;
            }
            b'-' => {
                prev?;
                range = true;
                i += 1;
            }
            _ => {
                let wd = weekday_token(s.get(i..i + 2)?)?;
                if range {
                    let mut d = prev?;
                    while d != wd {
                        d = (d + 1) % 7;
                        mask |= 1 << d;
                    }
                    range = false;
                }
                mask |= 1 << wd;
                prev = Some(wd);
                i += 2;
            }
        }
    }
    (!range && mask != 0).then_some(mask)
}

/// Parse the Days column.
pub fn parse_days(s: &str) -> DaysField {
    let s = s.trim();
    if s.is_empty() {
        return DaysField::Days(Days::Daily);
    }
    let lower = s.to_ascii_lowercase();
    if ["alt", "harm", "imod", "spur", "lsb", "usb"].contains(&lower.as_str()) {
        return DaysField::NotBroadcast;
    }
    if lower == "irr" {
        return DaysField::Days(Days::Irregular);
    }
    // 1245: weekdays by number, 1 = Monday
    if s.bytes().all(|b| (b'1'..=b'7').contains(&b)) {
        let mask = s.bytes().fold(0u8, |m, b| m | 1 << (b - b'1'));
        return DaysField::Days(Days::Weekly { mask });
    }
    if let Some(mask) = weekday_list(s) {
        return DaysField::Days(Days::Weekly { mask });
    }
    // 1.Sa: first Saturday of the month
    if let Some((n, wd)) = s.split_once('.') {
        if let (Ok(n @ 1..=5), Some(wd)) = (n.parse::<u8>(), weekday_token(wd)) {
            return DaysField::Days(Days::Nth { n, wd });
        }
    }
    // Last7: last Sunday of the month
    if let Some(d) = lower.strip_prefix("last") {
        if let Ok(d @ 1..=7) = d.parse::<u8>() {
            return DaysField::Days(Days::Last { wd: d - 1 });
        }
    }
    // 15Sep: one date
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if let (Ok(d @ 1..=31), Some(m)) = (s[..split].parse::<u8>(), MONTHS.iter().position(|m| lower[split..] == **m)) {
        return DaysField::Days(Days::Date { d, m: m as u8 + 1 });
    }
    DaysField::Unknown(s.to_string())
}

/// The Remarks column as `(host country, site code)`: empty = the station's country and
/// its default site, `k` = site `k` at home, `/BUL-s` = site `s` in Bulgaria, `/TWN` = the
/// default site in Taiwan.
pub fn parse_site_code<'a>(remarks: &'a str, station_itu: &'a str) -> (&'a str, Option<&'a str>) {
    let r = remarks.trim();
    let nonempty = |s: &'a str| Some(s).filter(|s| !s.is_empty());
    match r.strip_prefix('/') {
        Some(rest) => match rest.split_once('-') {
            Some((itu, code)) => (itu, nonempty(code)),
            None => (rest, None),
        },
        None => (station_itu, nonempty(r.trim_start_matches('-'))),
    }
}

/// The first coordinate pair in a README site line, as decimal degrees rounded to 4 places,
/// plus the byte offset where it starts. Accepts `26S35-28E08`, `26S07'40"-28E12'20"` and
/// the README's small variations (a missing dash, a space after the hemisphere, `'` for `"`).
pub fn parse_coord(s: &str) -> Option<(f64, f64, usize)> {
    let b = s.as_bytes();
    (0..b.len()).filter(|&i| b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit())).find_map(|i| {
        let (lat, j) = dms(s, i, b"NS", 2)?;
        let mut j = j;
        while j < b.len() && (b[j] == b'-' || b[j] == b' ') {
            j += 1;
        }
        let (lon, _) = dms(s, j, b"EW", 3)?;
        Some((round4(lat), round4(lon), i))
    })
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// `DD[NS]MM['SS"]` from byte `i`: degrees (signed) and the index after it.
fn dms(s: &str, i: usize, hemis: &[u8; 2], max_deg_digits: usize) -> Option<(f64, usize)> {
    let b = s.as_bytes();
    let digits = |from: usize, max: usize| {
        let n = b[from..].iter().take(max).take_while(|c| c.is_ascii_digit()).count();
        (n > 0).then(|| (s[from..from + n].parse::<f64>().unwrap_or(0.0), from + n))
    };
    let (deg, mut j) = digits(i, max_deg_digits)?;
    let hemi = *b.get(j)?;
    if !hemis.contains(&hemi) {
        return None;
    }
    j += 1;
    if b.get(j) == Some(&b' ') {
        j += 1;
    }
    let (min, mut j) = digits(j, 2)?;
    let mut sec = 0.0;
    if b.get(j) == Some(&b'\'') {
        if let Some((s2, k)) = digits(j + 1, 2) {
            sec = s2;
            j = k;
            if matches!(b.get(j), Some(b'"') | Some(b'\'')) {
                j += 1;
            }
        } else {
            j += 1;
        }
    }
    // a stray hemisphere letter after the seconds (`35N26'47"N-140E01'11"`)
    if b.get(j) == Some(&hemi) {
        j += 1;
    }
    if min >= 60.0 || sec >= 60.0 {
        return None;
    }
    let v = deg + min / 60.0 + sec / 3600.0;
    Some((if hemi == hemis[1] { -v } else { v }, j))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time("1700-1800"), Some((1020, 1080)));
        assert_eq!(parse_time("0000-2400"), Some((0, 1440)));
        assert_eq!(parse_time("2300-0100"), Some((1380, 60)));
        assert_eq!(parse_time("2400-0100"), None);
        assert_eq!(parse_time("1760-1800"), None);
        assert_eq!(parse_time("17:00-18:00"), None);
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn frequencies() {
        assert_eq!(parse_khz("5900"), Some(5_900_000));
        assert_eq!(parse_khz("7277.5"), Some(7_277_500));
        assert_eq!(parse_khz("abc"), None);
        assert_eq!(parse_khz("-5"), None);
    }

    #[test]
    fn persistence() {
        assert_eq!(parse_persistence(""), Some(0));
        assert_eq!(parse_persistence("6"), Some(6));
        assert_eq!(parse_persistence("98"), Some(98));
        assert_eq!(parse_persistence("x"), None);
    }

    #[test]
    fn dates_and_last_heard() {
        assert_eq!(parse_ddmm("2903"), Some((29, 3)));
        assert_eq!(parse_ddmm("1104[0426]"), Some((11, 4)));
        assert_eq!(parse_ddmm("99999"), None);
        assert_eq!(parse_ddmm("[0426]"), None);
        assert_eq!(parse_ddmm(""), None);
        assert_eq!(parse_last_heard("1104[0426]"), Some("2026-04".into()));
        assert_eq!(parse_last_heard("[0923]"), Some("2023-09".into()));
        assert_eq!(parse_last_heard("[1323]"), None);
        assert_eq!(parse_last_heard("1104"), None);
    }

    #[test]
    fn days() {
        use DaysField::*;
        assert_eq!(parse_days(""), Days(super::Days::Daily));
        assert_eq!(parse_days("Mo-Fr"), Days(super::Days::Weekly { mask: 0b001_1111 }));
        assert_eq!(parse_days("Tu,Fr"), Days(super::Days::Weekly { mask: 0b001_0010 }));
        assert_eq!(parse_days("SaSu"), Days(super::Days::Weekly { mask: 0b110_0000 }));
        assert_eq!(parse_days("We-Mo"), Days(super::Days::Weekly { mask: 0b111_1101 }));
        assert_eq!(parse_days("Su"), Days(super::Days::Weekly { mask: 0b100_0000 }));
        assert_eq!(parse_days("1245"), Days(super::Days::Weekly { mask: 0b001_1011 }));
        assert_eq!(parse_days("1.Sa"), Days(super::Days::Nth { n: 1, wd: 5 }));
        assert_eq!(parse_days("Last7"), Days(super::Days::Last { wd: 6 }));
        assert_eq!(parse_days("15Sep"), Days(super::Days::Date { d: 15, m: 9 }));
        assert_eq!(parse_days("5Apr"), Days(super::Days::Date { d: 5, m: 4 }));
        assert_eq!(parse_days("irr"), Days(super::Days::Irregular));
        assert_eq!(parse_days("alt"), NotBroadcast);
        assert_eq!(parse_days("USB"), NotBroadcast);
        assert_eq!(parse_days("spur"), NotBroadcast);
        assert_eq!(parse_days("Test"), Unknown("Test".into()));
        assert_eq!(parse_days("Mo-"), Unknown("Mo-".into()));
    }

    #[test]
    fn site_codes() {
        assert_eq!(parse_site_code("", "KOR"), ("KOR", None));
        assert_eq!(parse_site_code("ka", "CHN"), ("CHN", Some("ka")));
        assert_eq!(parse_site_code("/BUL-s", "CLA"), ("BUL", Some("s")));
        assert_eq!(parse_site_code("/TWN", "KOR"), ("TWN", None));
        assert_eq!(parse_site_code("-pr", "USA"), ("USA", Some("pr")));
    }

    #[test]
    fn coordinates() {
        assert_eq!(parse_coord("Meyerton 26S35-28E08 except:"), Some((-26.5833, 28.1333, 9)));
        assert_eq!(parse_coord("Johannesburg 26S07'40\"-28E12'20\"").map(|c| (c.0, c.1)), Some((-26.1278, 28.2056)));
        assert_eq!(parse_coord("Adrar 27N52-00W17").map(|c| (c.0, c.1)), Some((27.8667, -0.2833)));
        assert_eq!(parse_coord("Annette 55N03-131W34").map(|c| (c.0, c.1)), Some((55.05, -131.5667)));
        // README quirks
        assert_eq!(
            parse_coord("VKS737 Charter Towers QLD 20S05'06\"146E15'34\"").map(|c| (c.0, c.1)),
            Some((-20.085, 146.2594))
        );
        assert_eq!(
            parse_coord("RFDS Mount Isa QLD 20S 43'31\"-139E29'14\"").map(|c| (c.0, c.1)),
            Some((-20.7253, 139.4872))
        );
        assert_eq!(parse_coord("Muan HFDL 35N1'56\"-126E14'19\"").map(|c| (c.0, c.1)), Some((35.0322, 126.2386)));
        assert_eq!(parse_coord("(WWRB) 35N37'27'-86W00'52\"").map(|c| (c.0, c.1)), Some((35.6242, -86.0144)));
        assert_eq!(parse_coord("Kubota 35N26'47\"N-140E01'11\"").map(|c| (c.0, c.1)), Some((35.4464, 140.0197)));
        assert_eq!(parse_coord("Unknown site"), None);
        assert_eq!(parse_coord("(1x100kW = 2x50kW) 41N20-19E33").map(|c| (c.0, c.1)), Some((41.3333, 19.55)));
    }
}
