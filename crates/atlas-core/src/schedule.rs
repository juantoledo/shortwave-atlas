//! Broadcast schedules. A `Window` is a daily UTC span in minutes since 00:00 UTC
//! (0..=1440): `start > end` crosses midnight, `[0, 1440]` is all day. A `Slot` is one EiBi
//! entry: a window plus the days, season and dates it applies to, evaluated at a Unix time.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::calendar::{civil_from_days, day_of, days_in_month, is_northern_summer, switch_at, weekday};

pub const DAY_MIN: u32 = 1440;

/// One window in minutes since 00:00 UTC.
pub type Window = [u16; 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AirStatus {
    pub on: bool,
    /// Minutes until the status changes (off if `on`, on otherwise); `None` = never.
    pub minutes: Option<u32>,
    /// UTC minute of the next change (meaningless when `minutes` is `None`).
    pub edge: u32,
}

/// Which days a slot runs on (weekdays: Monday = 0 .. Sunday = 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "t", rename_all = "snake_case")]
#[ts(export)]
pub enum Days {
    Daily,
    /// Bit `n` set = weekday `n`.
    Weekly {
        mask: u8,
    },
    /// The `n`th weekday `wd` of the month (`1.Sa`).
    Nth {
        n: u8,
        wd: u8,
    },
    /// The last weekday `wd` of the month (`Last7`).
    Last {
        wd: u8,
    },
    /// One date every year (`15Sep`).
    Date {
        d: u8,
        m: u8,
    },
    /// Irregular, tests, special events: shown as possibly on during its hours.
    Irregular,
}

impl Days {
    /// Whether the pattern includes day number `day` (days since 1970-01-01).
    pub fn includes(&self, day: i32) -> bool {
        let wd = weekday(day);
        let (y, m, d) = civil_from_days(day);
        match *self {
            Days::Daily | Days::Irregular => true,
            Days::Weekly { mask } => mask & (1 << wd) != 0,
            Days::Nth { n, wd: w } => wd == w && (d - 1) / 7 + 1 == u32::from(n),
            Days::Last { wd: w } => wd == w && d + 7 > days_in_month(y, m),
            Days::Date { d: dd, m: mm } => d == u32::from(dd) && m == u32::from(mm),
        }
    }
}

/// EiBi persistence 4 / 5: only in the northern winter / summer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SeasonOnly {
    #[default]
    All,
    Winter,
    Summer,
}

impl SeasonOnly {
    fn active(self, unix: i64) -> bool {
        match self {
            SeasonOnly::All => true,
            SeasonOnly::Winter => !is_northern_summer(unix),
            SeasonOnly::Summer => is_northern_summer(unix),
        }
    }
}

/// One scheduled transmission (an EiBi row).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Slot {
    /// UTC minutes; `start > end` crosses midnight, `end` 1440 = midnight.
    pub start: u16,
    pub end: u16,
    pub days: Days,
    /// EiBi language code(s), e.g. `E` or `S,Q`.
    pub lang: String,
    /// EiBi target-area code, e.g. `CAm`, or a country code.
    pub target: String,
    pub season: SeasonOnly,
    /// First and last valid day (days since 1970-01-01), for EiBi persistence 6.
    pub from: Option<i32>,
    pub to: Option<i32>,
    /// Last logged, `YYYY-MM`.
    pub heard: Option<String>,
}

impl Slot {
    /// Whether the slot (as started on `day`) applies: days pattern and validity dates.
    fn runs_on(&self, day: i32) -> bool {
        self.days.includes(day) && self.from.is_none_or(|f| day >= f) && self.to.is_none_or(|t| day <= t)
    }

    /// The day this slot started on if it is on the air at `unix`.
    fn started(&self, unix: i64) -> Option<i32> {
        let (s, e) = (u32::from(self.start), u32::from(self.end));
        let day = day_of(unix);
        let m = (unix.div_euclid(60).rem_euclid(i64::from(DAY_MIN))) as u32;
        let start_day = if s < e {
            (m >= s && m < e).then_some(day)?
        } else if s > e {
            if m >= s {
                day
            } else if m < e {
                day - 1
            } else {
                return None;
            }
        } else {
            return None;
        };
        Some(start_day)
    }
}

/// Whether `slot` is on the air at `unix`. The days pattern and dates apply to the day the
/// slot starts, so a Monday 23:00-01:00 slot is still on at Tuesday 00:30.
pub fn slot_on(slot: &Slot, unix: i64) -> bool {
    slot.season.active(unix) && slot.started(unix).is_some_and(|d| slot.runs_on(d))
}

/// Whether any slot is on the air at `unix`.
pub fn any_on(slots: &[Slot], unix: i64) -> bool {
    slots.iter().any(|s| slot_on(s, unix))
}

/// How far ahead `air_status_at` looks for the next change.
const HORIZON_DAYS: i32 = 400;

/// Whether `slots` are on the air at `unix`, and when that next changes (to the minute).
pub fn air_status_at(slots: &[Slot], unix: i64) -> AirStatus {
    let now_min = unix.div_euclid(60);
    let on = any_on(slots, now_min * 60);
    let day0 = day_of(unix);
    let year = civil_from_days(day0).0;
    let switches: Vec<i64> =
        (year..=year + 2).flat_map(|y| [switch_at(y, 3), switch_at(y, 10)]).map(|t| t / 60).collect();
    let mut times = Vec::new();
    for k in 0..HORIZON_DAYS {
        let base = i64::from(day0 + k) * i64::from(DAY_MIN);
        times.clear();
        for s in slots {
            for m in [s.start, s.end] {
                times.push(base + i64::from(m));
            }
        }
        times.extend(switches.iter().copied().filter(|&t| t >= base && t < base + i64::from(DAY_MIN)));
        times.retain(|&t| t > now_min);
        times.sort_unstable();
        times.dedup();
        if let Some(&t) = times.iter().find(|&&t| any_on(slots, t * 60) != on) {
            let minutes = (t - now_min) as u32;
            return AirStatus { on, minutes: Some(minutes), edge: (t.rem_euclid(i64::from(DAY_MIN))) as u32 };
        }
    }
    AirStatus { on, minutes: None, edge: (now_min.rem_euclid(i64::from(DAY_MIN))) as u32 }
}

/// Today's (UTC) windows for a 24 h bar: slots that run today, plus the part of yesterday's
/// slots that spills past midnight. Sorted, with overlaps merged.
pub fn today_parts(slots: &[Slot], unix: i64) -> Vec<Window> {
    let day = day_of(unix);
    let mut out = Vec::new();
    for s in slots.iter().filter(|s| s.season.active(unix)) {
        if s.runs_on(day) {
            match s.start.cmp(&s.end) {
                std::cmp::Ordering::Less => out.push([s.start, s.end]),
                std::cmp::Ordering::Greater => out.push([s.start, DAY_MIN as u16]),
                std::cmp::Ordering::Equal => {}
            }
        }
        if s.start > s.end && s.end > 0 && s.runs_on(day - 1) {
            out.push([0, s.end]);
        }
    }
    out.sort_unstable();
    let mut merged: Vec<Window> = Vec::with_capacity(out.len());
    for w in out {
        match merged.last_mut() {
            Some(last) if w[0] <= last[1] => last[1] = last[1].max(w[1]),
            _ => merged.push(w),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::{days_from_civil, SECS_PER_DAY};

    /// Unix time for a UTC date and time.
    fn at(y: i32, mo: u32, d: u32, h: i64, mi: i64) -> i64 {
        i64::from(days_from_civil(y, mo, d)) * SECS_PER_DAY + h * 3600 + mi * 60
    }

    fn slot(start: u16, end: u16, days: Days) -> Slot {
        Slot {
            start,
            end,
            days,
            lang: "E".into(),
            target: "Eu".into(),
            season: SeasonOnly::All,
            from: None,
            to: None,
            heard: None,
        }
    }

    const MON: u8 = 1;

    #[test]
    fn inside_and_before_a_window() {
        let s = [slot(540, 1380, Days::Daily)];
        let day = |h, m| at(2026, 10, 8, h, m);
        assert_eq!(air_status_at(&s, day(10, 0)), AirStatus { on: true, minutes: Some(780), edge: 1380 });
        assert_eq!(air_status_at(&s, day(23, 20)), AirStatus { on: false, minutes: Some(580), edge: 540 });
        // the end is exclusive
        assert!(slot_on(&s[0], day(9, 0)));
        assert!(!slot_on(&s[0], day(23, 0)));
    }

    #[test]
    fn window_across_midnight() {
        let s = [slot(1200, 240, Days::Daily)];
        assert!(slot_on(&s[0], at(2026, 10, 8, 23, 59)));
        assert!(slot_on(&s[0], at(2026, 10, 9, 0, 0)));
        assert_eq!(air_status_at(&s, at(2026, 10, 9, 3, 59)), AirStatus { on: true, minutes: Some(1), edge: 240 });
        assert!(!slot_on(&s[0], at(2026, 10, 9, 4, 0)));
    }

    #[test]
    fn all_day_never_changes() {
        let s = [slot(0, 1440, Days::Daily)];
        assert_eq!(air_status_at(&s, at(2026, 10, 8, 11, 40)), AirStatus { on: true, minutes: None, edge: 700 });
    }

    #[test]
    fn no_slots_is_always_off() {
        assert_eq!(air_status_at(&[], at(2026, 10, 8, 0, 10)), AirStatus { on: false, minutes: None, edge: 10 });
    }

    #[test]
    fn seconds_round_down_to_the_minute() {
        let s = [slot(540, 1380, Days::Daily)];
        assert_eq!(air_status_at(&s, at(2026, 10, 8, 10, 0) + 30).minutes, Some(780));
    }

    #[test]
    fn starts_later_today() {
        // Thursday 21:00, a daily 21:30 slot
        let s = [slot(1290, 1350, Days::Daily)];
        assert_eq!(air_status_at(&s, at(2026, 10, 8, 21, 0)), AirStatus { on: false, minutes: Some(30), edge: 1290 });
    }

    #[test]
    fn monday_only_waits_for_monday() {
        // Thursday 2026-10-08 21:00 -> Monday 2026-10-12 21:30
        let s = [slot(1290, 1350, Days::Weekly { mask: MON })];
        let st = air_status_at(&s, at(2026, 10, 8, 21, 0));
        assert!(!st.on);
        assert_eq!(st.minutes, Some(4 * 1440 + 30));
        assert_eq!(st.edge, 1290);
    }

    #[test]
    fn next_start_after_midnight() {
        let s = [slot(0, 60, Days::Daily)];
        assert_eq!(air_status_at(&s, at(2026, 10, 8, 23, 30)).minutes, Some(30));
    }

    #[test]
    fn days_apply_to_the_starting_day() {
        let s = slot(1380, 60, Days::Weekly { mask: MON });
        assert!(slot_on(&s, at(2026, 10, 12, 23, 30))); // Monday
        assert!(slot_on(&s, at(2026, 10, 13, 0, 30))); // Tuesday, started Monday
        assert!(!slot_on(&s, at(2026, 10, 13, 23, 30)));
        assert!(!slot_on(&s, at(2026, 10, 12, 0, 30))); // Monday, started Sunday
    }

    #[test]
    fn same_start_and_end_is_never_on() {
        let s = slot(600, 600, Days::Daily);
        assert!(!slot_on(&s, at(2026, 10, 8, 10, 0)));
        assert_eq!(air_status_at(&[s], at(2026, 10, 8, 10, 0)).minutes, None);
    }

    #[test]
    fn nth_last_and_date_patterns() {
        let first_sat = Days::Nth { n: 1, wd: 5 };
        assert!(first_sat.includes(days_from_civil(2026, 10, 3)));
        assert!(!first_sat.includes(days_from_civil(2026, 10, 10)));
        let last_sun = Days::Last { wd: 6 };
        assert!(last_sun.includes(days_from_civil(2026, 10, 25)));
        assert!(!last_sun.includes(days_from_civil(2026, 10, 18)));
        let sep15 = Days::Date { d: 15, m: 9 };
        assert!(sep15.includes(days_from_civil(2026, 9, 15)));
        assert!(!sep15.includes(days_from_civil(2026, 9, 16)));
    }

    #[test]
    fn season_only_slots_follow_the_switch() {
        let mut s = slot(0, 1440, Days::Daily);
        s.season = SeasonOnly::Summer;
        let switch = switch_at(2026, 10);
        assert!(slot_on(&s, switch - 60));
        assert!(!slot_on(&s, switch));
        let st = air_status_at(std::slice::from_ref(&s), switch - 600);
        assert_eq!(st, AirStatus { on: true, minutes: Some(10), edge: 60 });
        s.season = SeasonOnly::Winter;
        assert!(!slot_on(&s, switch_at(2026, 3)));
        assert!(slot_on(&s, switch_at(2026, 3) - 60));
    }

    #[test]
    fn validity_dates_are_inclusive() {
        let mut s = slot(600, 660, Days::Daily);
        s.from = Some(days_from_civil(2026, 7, 5));
        s.to = Some(days_from_civil(2026, 7, 5));
        assert!(slot_on(&s, at(2026, 7, 5, 10, 30)));
        assert!(!slot_on(&s, at(2026, 7, 4, 10, 30)));
        assert!(!slot_on(&s, at(2026, 7, 6, 10, 30)));
        assert_eq!(air_status_at(&[s], at(2026, 7, 6, 10, 30)).minutes, None);
    }

    #[test]
    fn irregular_is_on_during_its_hours() {
        let s = slot(0, 1440, Days::Irregular);
        assert!(slot_on(&s, at(2026, 10, 8, 3, 0)));
    }

    #[test]
    fn today_includes_yesterdays_spill() {
        // Monday-only 23:00-01:00 seen on Tuesday: only the 00:00-01:00 part
        let s = [slot(1380, 60, Days::Weekly { mask: MON }), slot(600, 700, Days::Daily), slot(650, 720, Days::Daily)];
        assert_eq!(today_parts(&s, at(2026, 10, 13, 12, 0)), vec![[0, 60], [600, 720]]);
        assert_eq!(today_parts(&s, at(2026, 10, 12, 12, 0)), vec![[600, 720], [1380, 1440]]);
    }
}
