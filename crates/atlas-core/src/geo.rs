//! Great-circle math on a spherical Earth (port of the prototype's `//<pure>` block).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const EARTH_RADIUS_KM: f64 = 6371.0;
const SPEED_OF_LIGHT_KM_S: f64 = 299_792.458;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

/// Distance and initial bearing from one point to another.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Route {
    pub km: f64,
    /// Central angle in degrees.
    pub deg: f64,
    /// Initial bearing, 0..360 clockwise from true north.
    pub bearing: f64,
}

/// Haversine distance plus initial bearing from `a` to `b`.
pub fn dist_bearing(a: LatLon, b: LatLon) -> Route {
    let (p1, p2) = (a.lat.to_radians(), b.lat.to_radians());
    let dl = (b.lon - a.lon).to_radians();
    let s = ((p2 - p1) / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    let ang = 2.0 * s.sqrt().min(1.0).asin();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    Route { km: ang * EARTH_RADIUS_KM, deg: ang.to_degrees(), bearing: (y.atan2(x).to_degrees() + 360.0) % 360.0 }
}

/// The point where the Sun is overhead at `unix` (low-precision solar ephemeris, about
/// 0.1 degree; the longitude includes the equation of time).
pub fn subsolar(unix: i64) -> LatLon {
    // days since J2000.0 (2000-01-01 12:00 UTC)
    let d = unix as f64 / 86_400.0 - 10_957.5;
    let g = (357.529 + 0.985_600_28 * d).to_radians();
    let q = 280.459 + 0.985_647_36 * d;
    let l = (q + 1.915 * g.sin() + 0.020 * (2.0 * g).sin()).to_radians();
    let e = (23.439 - 0.000_000_36 * d).to_radians();
    let ra = (e.cos() * l.sin()).atan2(l.cos()).to_degrees();
    let dec = (e.sin() * l.sin()).asin().to_degrees();
    let gmst_deg = (280.460_618_37 + 360.985_647_366_29 * d).rem_euclid(360.0);
    LatLon { lat: dec, lon: (ra - gmst_deg + 540.0).rem_euclid(360.0) - 180.0 }
}

/// Day, twilight or night where something is, by the Sun's elevation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SunPhase {
    Day,
    /// Sun below the horizon by up to 12 degrees (civil and nautical twilight).
    Twilight,
    Night,
}

/// The Sun as seen from a place.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SunAt {
    /// Degrees above the horizon (negative = below).
    pub elevation: f64,
    pub phase: SunPhase,
    /// Apparent solar time in minutes (720 = the Sun is due south/north).
    pub solar_min: u16,
}

pub fn sun_at(pos: LatLon, unix: i64) -> SunAt {
    let sun = subsolar(unix);
    let elevation = 90.0 - dist_bearing(pos, sun).deg;
    // refraction and the Sun's radius put sunrise at -0.833 degrees
    let phase = if elevation > -0.833 {
        SunPhase::Day
    } else if elevation > -12.0 {
        SunPhase::Twilight
    } else {
        SunPhase::Night
    };
    let solar_min = (720.0 + (pos.lon - sun.lon) * 4.0).rem_euclid(1440.0).round() as u16 % 1440;
    SunAt { elevation, phase, solar_min }
}

/// The point a fraction `f` (0..=1) of the way from `a` to `b` along the great circle. Antipodes
/// have no single great circle: `a` is returned for them, as for one point.
pub fn along(a: LatLon, b: LatLon, f: f64) -> LatLon {
    let d = dist_bearing(a, b).deg.to_radians();
    if d < 1e-9 || std::f64::consts::PI - d < 1e-9 {
        return a;
    }
    let (la1, lo1, la2, lo2) = (a.lat.to_radians(), a.lon.to_radians(), b.lat.to_radians(), b.lon.to_radians());
    let (ka, kb) = (((1.0 - f) * d).sin() / d.sin(), (f * d).sin() / d.sin());
    let x = ka * la1.cos() * lo1.cos() + kb * la2.cos() * lo2.cos();
    let y = ka * la1.cos() * lo1.sin() + kb * la2.cos() * lo2.sin();
    let z = ka * la1.sin() + kb * la2.sin();
    LatLon { lat: z.atan2(x.hypot(y)).to_degrees(), lon: y.atan2(x).to_degrees() }
}

/// The share of the path from `a` to `b` (9 points, ends included) where the Sun is down.
pub fn dark_share(a: LatLon, b: LatLon, unix: i64) -> f32 {
    const N: usize = 9;
    let down = (0..N).filter(|&i| sun_at(along(a, b, i as f64 / (N - 1) as f64), unix).phase != SunPhase::Day).count();
    down as f32 / N as f32
}

/// Wavelength in metres for a frequency in Hz.
pub fn wavelength_m(freq_hz: u32) -> f64 {
    SPEED_OF_LIGHT_KM_S * 1000.0 / f64::from(freq_hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SANTIAGO: LatLon = LatLon { lat: -33.45, lon: -70.67 };
    const GREENVILLE: LatLon = LatLon { lat: 35.47, lon: -77.19 };

    #[test]
    fn santiago_to_greenville() {
        let r = dist_bearing(SANTIAGO, GREENVILLE);
        assert!((r.km - 7693.5).abs() < 1.0, "km = {}", r.km);
        assert!((r.bearing - 354.32).abs() < 0.01, "bearing = {}", r.bearing);
    }

    #[test]
    fn same_point_is_zero() {
        let r = dist_bearing(SANTIAGO, SANTIAGO);
        assert!(r.km.abs() < 1e-9);
    }

    #[test]
    fn antipodes_are_half_the_globe() {
        let r = dist_bearing(LatLon { lat: 0.0, lon: 0.0 }, LatLon { lat: 0.0, lon: 180.0 });
        assert!((r.deg - 180.0).abs() < 1e-9);
    }

    #[test]
    fn due_east_on_the_equator() {
        let r = dist_bearing(LatLon { lat: 0.0, lon: 0.0 }, LatLon { lat: 0.0, lon: 10.0 });
        assert!((r.bearing - 90.0).abs() < 1e-9);
    }

    /// Unix time for a UTC date and time.
    fn at(y: i32, m: u32, d: u32, h: i64) -> i64 {
        i64::from(crate::calendar::days_from_civil(y, m, d)) * 86_400 + h * 3600
    }

    #[test]
    fn subsolar_point_follows_the_seasons() {
        // March equinox 2026 (Mar 20 14:46 UTC): declination ~0
        assert!(subsolar(at(2026, 3, 20, 15)).lat.abs() < 0.1);
        // June solstice: ~23.44 N, and near the Greenwich meridian at noon UTC
        let s = subsolar(at(2026, 6, 21, 12));
        assert!((s.lat - 23.44).abs() < 0.05, "{s:?}");
        assert!(s.lon.abs() < 1.0, "{s:?}");
        // December solstice
        assert!((subsolar(at(2026, 12, 21, 12)).lat + 23.44).abs() < 0.05);
        // six hours later the Sun is ~90 degrees further west
        assert!((subsolar(at(2026, 6, 21, 18)).lon + 90.0).abs() < 1.5);
    }

    #[test]
    fn sun_at_a_site() {
        // Greenville, NC at 17:00 UTC in June: early afternoon, high Sun
        let s = sun_at(GREENVILLE, at(2026, 6, 21, 17));
        assert_eq!(s.phase, SunPhase::Day);
        assert!(s.elevation > 60.0, "{s:?}");
        assert!((s.solar_min as i32 - (17 * 60 - 309)).abs() < 5, "{s:?}");
        // and at 05:00 UTC it is night
        assert_eq!(sun_at(GREENVILLE, at(2026, 6, 21, 5)).phase, SunPhase::Night);
        // Santiago at 10:30 UTC in June: just before sunrise
        let t = sun_at(SANTIAGO, at(2026, 6, 21, 11));
        assert_eq!(t.phase, SunPhase::Twilight, "{t:?}");
    }

    #[test]
    fn along_the_great_circle() {
        let (a, b) = (LatLon { lat: 0.0, lon: 0.0 }, LatLon { lat: 0.0, lon: 90.0 });
        let m = along(a, b, 0.5);
        assert!(m.lat.abs() < 1e-9 && (m.lon - 45.0).abs() < 1e-9, "{m:?}");
        // the ends, and the midpoint is as far from both
        let (s, e) = (along(SANTIAGO, GREENVILLE, 0.0), along(SANTIAGO, GREENVILLE, 1.0));
        assert!(dist_bearing(s, SANTIAGO).km < 1e-6 && dist_bearing(e, GREENVILLE).km < 1e-6);
        let mid = along(SANTIAGO, GREENVILLE, 0.5);
        let (x, y) = (dist_bearing(SANTIAGO, mid).km, dist_bearing(mid, GREENVILLE).km);
        assert!((x - y).abs() < 1e-6 && (x + y - dist_bearing(SANTIAGO, GREENVILLE).km).abs() < 1e-6);
        // over the pole
        let p = along(LatLon { lat: 45.0, lon: 0.0 }, LatLon { lat: 45.0, lon: 180.0 }, 0.5);
        assert!((p.lat - 90.0).abs() < 1e-9, "{p:?}");
        assert_eq!(along(a, LatLon { lat: 0.0, lon: 180.0 }, 0.5), a);
    }

    #[test]
    fn dark_share_of_a_path() {
        // June, 12 UTC: Greenwich is in daylight, the date line in the dark
        let t = at(2026, 6, 21, 12);
        assert_eq!(dark_share(LatLon { lat: 50.0, lon: 0.0 }, LatLon { lat: 40.0, lon: 10.0 }, t), 0.0);
        assert_eq!(dark_share(LatLon { lat: -10.0, lon: 170.0 }, LatLon { lat: 0.0, lon: -170.0 }, t), 1.0);
        let half = dark_share(LatLon { lat: 0.0, lon: 0.0 }, LatLon { lat: 0.0, lon: 179.0 }, t);
        assert!(half > 0.2 && half < 0.8, "{half}");
    }

    #[test]
    fn wavelength_of_10_mhz_is_about_30_m() {
        assert!((wavelength_m(10_000_000) - 29.979).abs() < 0.001);
    }
}
