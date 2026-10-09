//! Civil dates in UTC without a date crate: days since 1970-01-01, weekdays, the
//! northern-summer switch (last Sunday of March / October, 01:00 UTC) and EiBi seasons.

pub const SECS_PER_DAY: i64 = 86_400;

/// Days since 1970-01-01 for a proleptic Gregorian date (Hinnant's `days_from_civil`).
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i32 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `(year, month 1..=12, day 1..=31)` for days since 1970-01-01.
pub fn civil_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// Weekday, Monday = 0 .. Sunday = 6.
pub fn weekday(day: i32) -> u8 {
    // 1970-01-01 was a Thursday
    (day + 3).rem_euclid(7) as u8
}

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Day number of the last Sunday of a month.
pub fn last_sunday(y: i32, m: u32) -> i32 {
    let last = days_from_civil(y, m, days_in_month(y, m));
    last - i32::from((weekday(last) + 1) % 7)
}

/// Unix time of the northern summer-time switch: last Sunday of `m`, 01:00 UTC.
pub fn switch_at(y: i32, m: u32) -> i64 {
    i64::from(last_sunday(y, m)) * SECS_PER_DAY + 3600
}

/// Whether `unix` falls in the northern summer (EiBi's A season and P5 entries).
pub fn is_northern_summer(unix: i64) -> bool {
    let (y, _, _) = civil_from_days(day_of(unix));
    unix >= switch_at(y, 3) && unix < switch_at(y, 10)
}

/// Days since 1970-01-01 for a Unix time.
pub fn day_of(unix: i64) -> i32 {
    unix.div_euclid(SECS_PER_DAY) as i32
}

/// `YYYY-MM-DD` for a day number.
pub fn iso_date(day: i32) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// An EiBi broadcast season: A = summer (from the last Sunday of March), B = winter (from
/// the last Sunday of October until March of the next year).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Season {
    pub summer: bool,
    pub year: i32,
}

impl Season {
    /// From an EiBi file name such as `sked-a26.csv`.
    pub fn from_filename(name: &str) -> Option<Self> {
        let stem = name.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
        let code = stem.strip_prefix("sked-")?.strip_suffix(".csv")?;
        let (letter, yy) = code.split_at_checked(1)?;
        if yy.len() != 2 || !yy.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let summer = match letter {
            "a" => true,
            "b" => false,
            _ => return None,
        };
        Some(Self { summer, year: 2000 + yy.parse::<i32>().ok()? })
    }

    /// The season in force at `unix`.
    pub fn at(unix: i64) -> Self {
        let (y, _, _) = civil_from_days(day_of(unix));
        if unix < switch_at(y, 3) {
            Self { summer: false, year: y - 1 }
        } else if unix < switch_at(y, 10) {
            Self { summer: true, year: y }
        } else {
            Self { summer: false, year: y }
        }
    }

    /// `A26`, `B26`.
    pub fn code(&self) -> String {
        format!("{}{:02}", if self.summer { 'A' } else { 'B' }, self.year.rem_euclid(100))
    }

    /// `[start, end)` as Unix times.
    pub fn range(&self) -> (i64, i64) {
        if self.summer {
            (switch_at(self.year, 3), switch_at(self.year, 10))
        } else {
            (switch_at(self.year, 10), switch_at(self.year + 1, 3))
        }
    }

    /// The day number of an EiBi `DDMM` date inside this season (a B season's January to
    /// June dates belong to the next year).
    pub fn ddmm(&self, d: u32, m: u32) -> Option<i32> {
        let y = if !self.summer && m <= 6 { self.year + 1 } else { self.year };
        (1..=12).contains(&m).then_some(())?;
        (1..=days_in_month(y, m)).contains(&d).then(|| days_from_civil(y, m, d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        for z in [-1, 0, 59, 11_016, 20_000, 20_734, 60_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(iso_date(days_from_civil(2026, 10, 8)), "2026-10-08");
    }

    #[test]
    fn weekdays() {
        assert_eq!(weekday(0), 3); // Thursday
        assert_eq!(weekday(days_from_civil(2026, 10, 1)), 3);
        assert_eq!(weekday(days_from_civil(2026, 10, 25)), 6);
    }

    #[test]
    fn last_sundays_of_2026() {
        assert_eq!(iso_date(last_sunday(2026, 3)), "2026-03-29");
        assert_eq!(iso_date(last_sunday(2026, 10)), "2026-10-25");
        assert_eq!(iso_date(last_sunday(2027, 3)), "2027-03-28");
    }

    #[test]
    fn summer_switches_at_one_utc() {
        let s = switch_at(2026, 3);
        assert!(!is_northern_summer(s - 1));
        assert!(is_northern_summer(s));
        let w = switch_at(2026, 10);
        assert!(is_northern_summer(w - 1));
        assert!(!is_northern_summer(w));
    }

    #[test]
    fn seasons_from_names_and_times() {
        assert_eq!(Season::from_filename("data/sked-a26.csv"), Some(Season { summer: true, year: 2026 }));
        assert_eq!(Season::from_filename("SKED-B26.CSV").map(|s| s.code()), Some("B26".into()));
        assert_eq!(Season::from_filename("sked-c26.csv"), None);
        assert_eq!(Season::from_filename("other.csv"), None);
        assert_eq!(Season::at(switch_at(2026, 10)).code(), "B26");
        assert_eq!(Season::at(switch_at(2026, 10) - 1).code(), "A26");
        assert_eq!(Season::at(switch_at(2027, 3) - 1).code(), "B26");
    }

    #[test]
    fn b_season_dates_cross_new_year() {
        let b = Season { summer: false, year: 2026 };
        assert_eq!(b.ddmm(15, 11).map(iso_date), Some("2026-11-15".into()));
        assert_eq!(b.ddmm(4, 1).map(iso_date), Some("2027-01-04".into()));
        assert_eq!(b.ddmm(31, 2), None);
        let a = Season { summer: true, year: 2026 };
        assert_eq!(a.ddmm(5, 7).map(iso_date), Some("2026-07-05".into()));
        assert_eq!(a.ddmm(1, 13), None);
    }
}
