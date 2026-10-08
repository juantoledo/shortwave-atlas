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

    #[test]
    fn wavelength_of_10_mhz_is_about_30_m() {
        assert!((wavelength_m(10_000_000) - 29.979).abs() < 0.001);
    }
}
