//! SW Atlas core: pure logic with no IO (geo, schedules, station lookup, rig types,
//! and the per-OS rules, which take an `Os` argument so every variant is unit-tested).

pub mod api;
pub mod audio;
pub mod geo;
pub mod platform;
pub mod rig;
pub mod schedule;
pub mod setup;
pub mod stations;
pub mod tools;
pub mod update;

/// Minutes since 00:00 UTC for a Unix time in seconds.
pub fn utc_minute(unix_secs: u64) -> u32 {
    ((unix_secs / 60) % u64::from(schedule::DAY_MIN)) as u32
}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_minute_wraps_daily() {
        assert_eq!(super::utc_minute(0), 0);
        assert_eq!(super::utc_minute(86_400 + 9 * 3600 + 30 * 60 + 59), 570);
    }
}
