//! The bands on the dial: broadcast (the EiBi ones plus LW, MW and FM), amateur, CB and a few
//! utility services, with edges for the listener's ITU region; and the spans the whole dial shows
//! (only those the rig receives).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::eibi::names::BANDS;
use crate::geo::LatLon;
use crate::rig::RxRange;
use crate::scan::default_step_hz;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum BandKind {
    Broadcast,
    Ham,
    Cb,
    Utility,
}

/// A band on the dial.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Band {
    /// Unique: `49m` (broadcast, the station filter's ids), `ham-40m`, `cb`, `air`.
    pub id: String,
    /// What the dial prints on its bar: `49`, `40`, `70cm`, `CB`, `MW`.
    pub label: String,
    pub kind: BandKind,
    pub lo_khz: u32,
    pub hi_khz: u32,
    /// The step a scan of this band takes unless the listener picks another (see
    /// `scan::default_step_hz`).
    pub scan_step_hz: u32,
}

impl Band {
    /// `step_hz`: the band's channel spacing, 0 = not channelised.
    fn new(id: &str, label: &str, kind: BandKind, lo_khz: u32, hi_khz: u32, step_hz: u32) -> Self {
        let scan_step_hz = default_step_hz(step_hz, lo_khz, hi_khz);
        Self { id: id.into(), label: label.into(), kind, lo_khz, hi_khz, scan_step_hz }
    }

    /// An EiBi broadcast band (`49m` -> label `49`), on the 5 kHz HF broadcast raster.
    pub fn broadcast(id: &str, lo_khz: u32, hi_khz: u32) -> Self {
        Self::new(id, id.trim_end_matches('m'), BandKind::Broadcast, lo_khz, hi_khz, 5_000)
    }
}

/// ITU Radio Regulations region: 1 Europe, Africa, Middle East, ex-USSR; 2 the Americas;
/// 3 the rest of Asia and Oceania.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ItuRegion {
    R1,
    R2,
    R3,
}

/// Longitude of a boundary at a latitude, from its corners (north to south), linearly between them.
fn line(corners: &[(f64, f64)], lat: f64) -> f64 {
    for w in corners.windows(2) {
        let ((la, oa), (lb, ob)) = (w[0], w[1]);
        if lat <= la && lat >= lb {
            return oa + (la - lat) / (la - lb) * (ob - oa);
        }
    }
    corners.last().map_or(0.0, |c| c.1)
}

/// Lines A, B and C of RR 5.2–5.9, straightened between their corners (west is negative;
/// line C runs below -180 so it can cross the antimeridian).
const LINE_A: [(f64, f64); 4] = [(90.0, 40.0), (40.0, 40.0), (23.5, 60.0), (-90.0, 60.0)];
const LINE_B: [(f64, f64); 6] = [(90.0, -10.0), (72.0, -10.0), (65.0, -30.0), (40.0, -50.0), (-10.0, -20.0), (-90.0, -20.0)];
const LINE_C: [(f64, f64); 6] =
    [(90.0, -169.0), (65.5, -169.0), (50.0, -195.0), (10.0, -170.0), (10.0, -120.0), (-90.0, -120.0)];

impl ItuRegion {
    /// The region of a place. Approximate near the borders: the lines are straightened, and the
    /// ex-USSR part of Region 1 east of line A is taken as everything north of 50° N plus
    /// Central Asia and Mongolia as boxes.
    pub fn of(p: LatLon) -> Self {
        let west = if p.lon > 160.0 { p.lon - 360.0 } else { p.lon };
        if west < line(&LINE_B, p.lat) && west > line(&LINE_C, p.lat) {
            return Self::R2;
        }
        let ex_ussr = p.lat >= 50.0
            || (p.lat >= 36.5 && p.lon < 75.0)
            || (p.lat >= 41.0 && p.lon < 88.0)
            || (p.lat >= 42.0 && (88.0..120.0).contains(&p.lon));
        if (p.lon >= -30.0 && p.lon <= line(&LINE_A, p.lat)) || (p.lon > 0.0 && ex_ussr) {
            Self::R1
        } else {
            Self::R3
        }
    }
}

/// A stretch of the whole dial: `mf` (LW and MW), `hf` (shortwave), `vhf`, `uhf`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Span {
    pub id: String,
    pub lo_khz: u32,
    pub hi_khz: u32,
}

pub const SPANS: [(&str, u32, u32); 4] =
    [("mf", 100, 1_700), ("hf", 1_700, 30_000), ("vhf", 30_000, 300_000), ("uhf", 300_000, 1_000_000)];

fn overlaps(lo: u32, hi: u32, cov: &[RxRange]) -> bool {
    cov.iter().any(|r| r.lo_khz < hi && r.hi_khz > lo)
}

/// The spans that hold some of the rig's coverage.
pub fn spans_for(coverage: &[RxRange]) -> Vec<Span> {
    SPANS
        .iter()
        .filter(|s| overlaps(s.1, s.2, coverage))
        .map(|&(id, lo, hi)| Span { id: id.into(), lo_khz: lo, hi_khz: hi })
        .collect()
}

use BandKind::{Broadcast, Cb, Ham, Utility};
use ItuRegion::{R1, R2, R3};

/// One row of `OTHER_BANDS`: the regions it applies to (empty = all), the edges in kHz and the
/// channel spacing in Hz (0 = not channelised).
type Row = (&'static str, &'static str, BandKind, &'static [ItuRegion], u32, u32, u32);

/// Bands besides EiBi's shortwave ones. A band that differs by region has one row per variant.
/// Amateur edges are the ITU allocations; national rules may be narrower. AIR and marine are
/// scanned on their 25 kHz plan (8.33 kHz would take three times as long).
const OTHER_BANDS: &[Row] = &[
    ("ham-2200m", "2200", Ham, &[], 136, 138, 0),
    ("lw", "LW", Broadcast, &[R1], 148, 284, 9_000),
    ("ham-630m", "630", Ham, &[], 472, 479, 0),
    ("mw", "MW", Broadcast, &[R1, R3], 526, 1_606, 9_000),
    ("mw", "MW", Broadcast, &[R2], 525, 1_705, 10_000),
    ("ham-160m", "160", Ham, &[R1], 1_810, 2_000, 0),
    ("ham-160m", "160", Ham, &[R2, R3], 1_800, 2_000, 0),
    ("ham-80m", "80", Ham, &[R1], 3_500, 3_800, 0),
    ("ham-80m", "80", Ham, &[R2], 3_500, 4_000, 0),
    ("ham-80m", "80", Ham, &[R3], 3_500, 3_900, 0),
    ("ham-60m", "60", Ham, &[R1, R3], 5_351, 5_367, 0),
    ("ham-60m", "60", Ham, &[R2], 5_330, 5_407, 0),
    ("ham-40m", "40", Ham, &[R1, R3], 7_000, 7_200, 0),
    ("ham-40m", "40", Ham, &[R2], 7_000, 7_300, 0),
    ("ham-30m", "30", Ham, &[], 10_100, 10_150, 0),
    ("ham-20m", "20", Ham, &[], 14_000, 14_350, 0),
    ("ham-17m", "17", Ham, &[], 18_068, 18_168, 0),
    ("ham-15m", "15", Ham, &[], 21_000, 21_450, 0),
    ("ham-12m", "12", Ham, &[], 24_890, 24_990, 0),
    ("cb", "CB", Cb, &[], 26_965, 27_405, 10_000),
    ("ham-10m", "10", Ham, &[], 28_000, 29_700, 0),
    ("ham-6m", "6", Ham, &[R1], 50_000, 52_000, 0),
    ("ham-6m", "6", Ham, &[R2, R3], 50_000, 54_000, 0),
    ("ham-4m", "4", Ham, &[R1], 70_000, 70_500, 0),
    ("fm", "FM", Broadcast, &[], 87_500, 108_000, 100_000),
    ("air", "AIR", Utility, &[], 118_000, 137_000, 25_000),
    ("ham-2m", "2", Ham, &[R1], 144_000, 146_000, 0),
    ("ham-2m", "2", Ham, &[R2, R3], 144_000, 148_000, 0),
    ("marine", "MAR", Utility, &[], 156_000, 162_025, 25_000),
    ("ham-1.25m", "1.25", Ham, &[R2], 222_000, 225_000, 0),
    ("ham-70cm", "70cm", Ham, &[R1, R3], 430_000, 440_000, 0),
    ("ham-70cm", "70cm", Ham, &[R2], 420_000, 450_000, 0),
    ("pmr446", "PMR", Utility, &[R1], 446_000, 446_200, 12_500),
    ("frs", "FRS", Utility, &[R2], 462_550, 467_725, 25_000),
    ("ham-33cm", "33cm", Ham, &[R2], 902_000, 928_000, 0),
];

/// Every band for a region, sorted by lower edge.
pub fn dial_bands(region: ItuRegion) -> Vec<Band> {
    let mut v: Vec<Band> = BANDS.iter().map(|&(id, lo, hi)| Band::broadcast(id, lo, hi)).collect();
    v.extend(
        OTHER_BANDS
            .iter()
            .filter(|r| r.3.is_empty() || r.3.contains(&region))
            .map(|&(id, label, kind, _, lo, hi, step)| Band::new(id, label, kind, lo, hi, step)),
    );
    v.sort_by_key(|b| (b.lo_khz, b.hi_khz));
    v
}

/// Result of `Call::Dial`: what the whole dial and the band dial show for this rig and QTH.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Dial {
    pub region: ItuRegion,
    /// What the rig receives (or the fallback when it cannot say).
    pub coverage: Vec<RxRange>,
    pub spans: Vec<Span>,
    /// The region's bands that the rig receives at least in part, sorted by lower edge.
    pub bands: Vec<Band>,
}

impl Dial {
    pub fn new(region: ItuRegion, coverage: Vec<RxRange>) -> Self {
        let bands = dial_bands(region).into_iter().filter(|b| overlaps(b.lo_khz, b.hi_khz, &coverage)).collect();
        Self { region, spans: spans_for(&coverage), bands, coverage }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lat: f64, lon: f64) -> ItuRegion {
        ItuRegion::of(LatLon { lat, lon })
    }

    #[test]
    fn regions_of_places() {
        assert_eq!(at(-33.45, -70.67), R2, "Santiago");
        assert_eq!(at(61.2, -149.9), R2, "Anchorage");
        assert_eq!(at(21.3, -157.9), R2, "Honolulu");
        assert_eq!(at(51.9, -176.6), R2, "Adak, Aleutians");
        assert_eq!(at(64.2, -51.7), R2, "Nuuk");
        assert_eq!(at(-8.05, -34.9), R2, "Recife");
        assert_eq!(at(40.4, -3.7), R1, "Madrid");
        assert_eq!(at(64.1, -21.9), R1, "Reykjavik");
        assert_eq!(at(38.7, -27.2), R1, "Azores");
        assert_eq!(at(55.75, 37.6), R1, "Moscow");
        assert_eq!(at(55.0, 83.0), R1, "Novosibirsk");
        assert_eq!(at(43.2, 76.9), R1, "Almaty");
        assert_eq!(at(47.9, 106.9), R1, "Ulaanbaatar");
        assert_eq!(at(25.2, 55.3), R1, "Dubai");
        assert_eq!(at(-33.9, 18.4), R1, "Cape Town");
        assert_eq!(at(35.7, 139.7), R3, "Tokyo");
        assert_eq!(at(-33.87, 151.2), R3, "Sydney");
        assert_eq!(at(-41.3, 174.8), R3, "Wellington");
        assert_eq!(at(39.9, 116.4), R3, "Beijing");
        assert_eq!(at(28.6, 77.2), R3, "Delhi");
        assert_eq!(at(35.7, 51.4), R3, "Tehran");
        assert_eq!(at(-17.5, -149.6), R3, "Tahiti");
    }

    #[test]
    fn bands_per_region() {
        for r in [R1, R2, R3] {
            let b = dial_bands(r);
            let mut ids: Vec<&str> = b.iter().map(|b| b.id.as_str()).collect();
            ids.sort_unstable();
            let n = ids.len();
            ids.dedup();
            assert_eq!(ids.len(), n, "duplicate ids in {r:?}");
            assert!(b.windows(2).all(|w| w[0].lo_khz <= w[1].lo_khz));
            assert!(b.iter().all(|b| b.lo_khz < b.hi_khz));
        }
        let forty = |r| dial_bands(r).into_iter().find(|b| b.id == "ham-40m").unwrap();
        assert_eq!((forty(R1).lo_khz, forty(R1).hi_khz), (7_000, 7_200));
        assert_eq!(forty(R2).hi_khz, 7_300);
        assert!(dial_bands(R1).iter().any(|b| b.id == "lw"));
        assert!(!dial_bands(R2).iter().any(|b| b.id == "lw" || b.id == "pmr446"));
        let b49 = dial_bands(R2).into_iter().find(|b| b.id == "49m").unwrap();
        assert_eq!((b49.label.as_str(), b49.kind), ("49", Broadcast));
    }

    #[test]
    fn dial_follows_coverage() {
        let hf = Dial::new(R2, vec![RxRange { lo_khz: 30, hi_khz: 56_000 }]);
        let spans: Vec<&str> = hf.spans.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(spans, ["mf", "hf", "vhf"]);
        assert!(hf.bands.iter().any(|b| b.id == "ham-6m"));
        assert!(!hf.bands.iter().any(|b| b.id == "ham-2m" || b.id == "fm"));

        let vu = Dial::new(R1, vec![RxRange { lo_khz: 100_000, hi_khz: 180_000 }, RxRange { lo_khz: 400_000, hi_khz: 480_000 }]);
        let spans: Vec<&str> = vu.spans.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(spans, ["vhf", "uhf"]);
        let ids: Vec<&str> = vu.bands.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["fm", "air", "ham-2m", "marine", "ham-70cm", "pmr446"]);
    }
}
