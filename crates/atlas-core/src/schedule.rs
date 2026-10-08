//! Daily UTC broadcast windows, in minutes since 00:00 UTC (0..=1440).
//!
//! A window `[start, end]` with `start > end` crosses midnight; `[0, 1440]` is all day.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

/// Split windows that cross midnight into two, so every part has `start < end`.
pub fn window_parts(windows: &[Window]) -> Vec<Window> {
    let mut out = Vec::with_capacity(windows.len() + 1);
    for &[s, e] in windows {
        if s < e {
            out.push([s, e]);
        } else if s > e {
            out.push([s, DAY_MIN as u16]);
            if e > 0 {
                out.push([0, e]);
            }
        }
    }
    out
}

fn is_on(parts: &[Window], m: u32) -> bool {
    parts.iter().any(|&[s, e]| m >= u32::from(s) && m < u32::from(e))
}

/// Whether a station is on the air at UTC minute `m`, and when that changes.
pub fn air_status(windows: &[Window], m: u32) -> AirStatus {
    let parts = window_parts(windows);
    let m = m % DAY_MIN;
    let on = is_on(&parts, m);
    let k = (1..=DAY_MIN).find(|k| is_on(&parts, (m + k) % DAY_MIN) != on);
    AirStatus { on, minutes: k, edge: (m + k.unwrap_or(0)) % DAY_MIN }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_midnight_splits_in_two() {
        assert_eq!(window_parts(&[[1200, 240]]), vec![[1200, 1440], [0, 240]]);
        assert_eq!(window_parts(&[[1080, 0]]), vec![[1080, 1440]]);
        assert_eq!(window_parts(&[[600, 600]]), Vec::<Window>::new());
    }

    #[test]
    fn on_inside_a_window() {
        // Radio Martí sample: 09:00-23:00 UTC
        let s = air_status(&[[540, 1380]], 600);
        assert_eq!(s, AirStatus { on: true, minutes: Some(780), edge: 1380 });
    }

    #[test]
    fn off_before_a_window() {
        let s = air_status(&[[540, 1380]], 1400);
        assert_eq!(s, AirStatus { on: false, minutes: Some(580), edge: 540 });
    }

    #[test]
    fn window_across_midnight() {
        let w = [[1200, 240]];
        assert!(air_status(&w, 1439).on);
        assert!(air_status(&w, 0).on);
        let s = air_status(&w, 239);
        assert_eq!(s, AirStatus { on: true, minutes: Some(1), edge: 240 });
        assert!(!air_status(&w, 240).on);
    }

    #[test]
    fn all_day_never_changes() {
        let s = air_status(&[[0, 1440]], 700);
        assert_eq!(s, AirStatus { on: true, minutes: None, edge: 700 });
    }

    #[test]
    fn no_windows_is_always_off() {
        assert_eq!(air_status(&[], 10), AirStatus { on: false, minutes: None, edge: 10 });
    }

    #[test]
    fn end_is_exclusive() {
        assert!(!air_status(&[[540, 1380]], 1380).on);
        assert!(air_status(&[[540, 1380]], 540).on);
    }
}
