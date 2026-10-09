//! Simulated rig (the prototype's): remembers frequency/mode/power and fakes an S-meter
//! that is strong on stations that are on the air and weaker the farther they are.

use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use atlas_core::api::Qth;
use atlas_core::rig::{CommandError, Link, Mode, Power, RigState};
use atlas_core::stations::{Catalog, LOOKUP_TOLERANCE_HZ};

use crate::RigBackend;

struct SimRig {
    freq_hz: u32,
    mode: Mode,
    power: bool,
    noise: u64,
}

pub struct Sim {
    rig: Mutex<SimRig>,
    stations: Arc<Catalog>,
    qth: Arc<RwLock<Qth>>,
}

impl Sim {
    pub fn new(stations: Arc<Catalog>, qth: Arc<RwLock<Qth>>) -> Self {
        Self {
            rig: Mutex::new(SimRig { freq_hz: 13_570_000, mode: Mode::AM, power: true, noise: 0x2545_F491_4F6C_DD1D }),
            stations,
            qth,
        }
    }

    /// Fake S-meter in S-units (0..=11, above 9 meaning S9+10 dB steps).
    fn s_units(&self, freq_hz: u32, now_ms: u64, noise: f64) -> f64 {
        let qth = self.qth.read().expect("qth lock").pos();
        let best = self.stations.strongest_on_air(freq_hz, (now_ms / 1000) as i64, qth, LOOKUP_TOLERANCE_HZ);
        let lvl = match best {
            Some(route) => {
                let base = (9.4 - route.km / 2400.0).clamp(2.5, 8.6);
                base + (now_ms as f64 / 1700.0).sin() * 0.7 + (noise - 0.5) * 0.8
            }
            None => 0.4 + noise * 1.2,
        };
        lvl.clamp(0.0, 11.0)
    }
}

/// S-units to Hamlib `STRENGTH` (dB relative to S9; 6 dB per S-unit below S9).
pub fn s_units_to_db(s: f64) -> i32 {
    let db = if s <= 9.0 { (s - 9.0) * 6.0 } else { (s - 9.0) * 10.0 };
    db.round() as i32
}

fn passband(mode: Mode) -> u32 {
    match mode {
        Mode::AM => 6000,
        Mode::FM => 12000,
        Mode::USB | Mode::LSB => 2400,
        Mode::CW | Mode::CWR => 500,
    }
}

#[async_trait]
impl RigBackend for Sim {
    async fn get_state(&self, _last: &RigState) -> RigState {
        let (freq_hz, mode, power, noise) = {
            let mut r = self.rig.lock().expect("sim lock");
            // xorshift64: good enough for meter jitter, no extra crate
            r.noise ^= r.noise << 13;
            r.noise ^= r.noise >> 7;
            r.noise ^= r.noise << 17;
            (r.freq_hz, r.mode, r.power, (r.noise >> 11) as f64 / (1u64 << 53) as f64)
        };
        if !power {
            return RigState::without_rig(Link::Sim, Power::Off);
        }
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        RigState {
            link: Link::Sim,
            power: Power::On,
            freq_hz: Some(freq_hz),
            mode: Some(mode.as_str().to_string()),
            passband_hz: Some(passband(mode)),
            strength_db: Some(s_units_to_db(self.s_units(freq_hz, now_ms, noise))),
            cw_pitch_hz: matches!(mode, Mode::CW | Mode::CWR).then_some(600),
        }
    }

    async fn set_freq(&self, hz: u32) -> Result<(), CommandError> {
        self.rig.lock().expect("sim lock").freq_hz = hz;
        Ok(())
    }

    async fn set_mode(&self, mode: Mode) -> Result<(), CommandError> {
        self.rig.lock().expect("sim lock").mode = mode;
        Ok(())
    }

    async fn set_power(&self, on: bool) -> Result<(), CommandError> {
        self.rig.lock().expect("sim lock").power = on;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use atlas_core::eibi::{parse, EibiFiles};

    /// A one-station catalog: CFRX Toronto on 6070 kHz, 24 h a day.
    pub(crate) fn catalog() -> Arc<Catalog> {
        let readme = "   IV) Transmitter site codes.
   CAN: Toronto 43N30-79W38
";
        let csv = "kHz;Time(UTC);ITU;Station;Remarks;P
6070;0000-2400;CAN;CFRX Toronto;;1
";
        let files = EibiFiles {
            csv: csv.as_bytes(),
            csv_name: "sked-a26.csv",
            readme: readme.as_bytes(),
            overrides: "",
            utility: "",
        };
        Arc::new(Catalog::from_eibi(parse(&files).unwrap()))
    }

    fn sim() -> Sim {
        Sim::new(catalog(), Arc::new(RwLock::new(Qth::default())))
    }

    #[test]
    fn s_units_map_to_db() {
        assert_eq!(s_units_to_db(9.0), 0);
        assert_eq!(s_units_to_db(0.0), -54);
        assert_eq!(s_units_to_db(10.0), 10);
    }

    #[test]
    fn on_air_station_is_stronger_than_empty_band() {
        let s = sim();
        // CFRX 6070 kHz is on the air all day; 11.111 MHz has nothing
        assert!(s.s_units(6_070_000, 0, 0.5) > s.s_units(11_111_000, 0, 1.0));
    }

    #[tokio::test]
    async fn remembers_settings_and_power() {
        let s = sim();
        s.set_freq(5_025_000).await.unwrap();
        s.set_mode(Mode::CW).await.unwrap();
        let st = s.get_state(&RigState::down()).await;
        assert_eq!((st.link, st.freq_hz, st.cw_pitch_hz), (Link::Sim, Some(5_025_000), Some(600)));
        s.set_power(false).await.unwrap();
        assert_eq!(s.get_state(&st).await, RigState::without_rig(Link::Sim, Power::Off));
    }
}
