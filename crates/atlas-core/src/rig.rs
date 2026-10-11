//! Rig state and commands shared by the rig backends, the Tauri shell and the WebSocket.
//!
//! Receive only: there is deliberately no PTT or anything else that transmits.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tuning limits when the rig cannot say what it receives (FTDX10 general coverage).
pub const FREQ_MIN_HZ: u32 = 30_000;
pub const FREQ_MAX_HZ: u32 = 56_000_000;

/// A stretch the rig receives, in kHz (u32 Hz would stop at 4.29 GHz).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RxRange {
    pub lo_khz: u32,
    pub hi_khz: u32,
}

impl RxRange {
    pub fn contains_hz(&self, hz: u32) -> bool {
        (u64::from(self.lo_khz) * 1000..=u64::from(self.hi_khz) * 1000).contains(&u64::from(hz))
    }
}

/// The coverage assumed when the rig cannot report its own.
pub fn default_coverage() -> Vec<RxRange> {
    vec![RxRange { lo_khz: FREQ_MIN_HZ / 1000, hi_khz: FREQ_MAX_HZ / 1000 }]
}

/// The receive ranges in rigctld's `+\dump_state` answer: after the header lines (protocol,
/// model, ITU region) come `start end modes low_power high_power vfo ant` lines in Hz, ended
/// by an all-zero line (then the transmit ranges, which we ignore). Sorted and merged.
pub fn parse_dump_state(lines: &[String]) -> Vec<RxRange> {
    let mut out: Vec<RxRange> = vec![];
    let rows = lines.iter().map(|l| l.split_whitespace().collect::<Vec<_>>()).skip_while(|t| t.len() < 7);
    for t in rows {
        let (Some(lo), Some(hi)) =
            (t.first().and_then(|v| v.parse::<f64>().ok()), t.get(1).and_then(|v| v.parse::<f64>().ok()))
        else {
            break;
        };
        if t.len() < 7 || (lo == 0.0 && hi == 0.0) {
            break;
        }
        if hi > lo && lo >= 0.0 {
            let khz = |k: f64| k.min(f64::from(u32::MAX)) as u32;
            out.push(RxRange { lo_khz: khz((lo / 1000.0).floor()), hi_khz: khz((hi / 1000.0).ceil()) });
        }
    }
    out.sort_by_key(|r| r.lo_khz);
    out.into_iter().fold(vec![], |mut acc: Vec<RxRange>, r| {
        match acc.last_mut() {
            Some(last) if r.lo_khz <= last.hi_khz => last.hi_khz = last.hi_khz.max(r.hi_khz),
            _ => acc.push(r),
        }
        acc
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Power {
    On,
    Off,
    /// The rig did not answer (typically off, or the CAT cable is out).
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Link {
    /// Talking to rigctld.
    Ok,
    /// rigctld is not reachable.
    Down,
    /// Simulated rig.
    Sim,
    /// Disconnected on purpose from the settings page.
    Off,
}

/// Modes the UI may set. The rig may report others (e.g. PKTUSB); those come back as strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Mode {
    USB,
    LSB,
    CW,
    CWR,
    AM,
    /// Narrow AM (the FTDX10's AM-N).
    AMN,
    FM,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::USB => "USB",
            Self::LSB => "LSB",
            Self::CW => "CW",
            Self::CWR => "CWR",
            Self::AM => "AM",
            Self::AMN => "AMN",
            Self::FM => "FM",
        }
    }

    /// The rig's mode (as it reports it) already serves for this one: the same, or AM-N for
    /// AM (a broadcast keeps the narrow filter the listener chose).
    pub fn satisfied_by(self, current: Option<&str>) -> bool {
        current == Some(self.as_str()) || (self == Self::AM && current == Some(Self::AMN.as_str()))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RigState {
    pub link: Link,
    pub power: Power,
    /// The fields below are `None` unless the rig is on.
    pub freq_hz: Option<u32>,
    pub mode: Option<String>,
    pub passband_hz: Option<u32>,
    /// Signal level in dB relative to S9 (Hamlib `STRENGTH`).
    pub strength_db: Option<i32>,
    /// CW pitch, only in CW/CWR (the waterfall needs it for click-to-tune).
    pub cw_pitch_hz: Option<u32>,
}

impl RigState {
    pub fn down() -> Self {
        Self::without_rig(Link::Down, Power::Unknown)
    }

    pub fn without_rig(link: Link, power: Power) -> Self {
        Self { link, power, freq_hz: None, mode: None, passband_hz: None, strength_db: None, cw_pitch_hz: None }
    }
}

/// Commands from a UI (Tauri `invoke` or WebSocket) to the rig.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "cmd", rename_all = "snake_case")]
#[ts(export)]
pub enum RigCommand {
    SetFreq { hz: u32 },
    /// Tune to a station: its frequency, then its mode once the rig is there. A rig that
    /// changes band to get there may recall the mode it last used on that band (FTDX-10).
    Tune { hz: u32, mode: Mode },
    SetMode { mode: Mode },
    SetPower { on: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// Bad input (HTTP 400 equivalent).
    Invalid(String),
    /// Not allowed in the current state, e.g. the rig is off (HTTP 409 equivalent).
    Conflict(String),
    /// The rig or rigctld failed (HTTP 502 equivalent).
    Rig(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(m) | Self::Conflict(m) | Self::Rig(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for CommandError {}

impl RigCommand {
    /// Reject out-of-range input before anything reaches the rig: frequencies the rig does
    /// not receive (`coverage`).
    pub fn validate(&self, coverage: &[RxRange]) -> Result<(), CommandError> {
        match *self {
            Self::SetFreq { hz } | Self::Tune { hz, .. } if !coverage.iter().any(|r| r.contains_hz(hz)) => {
                Err(CommandError::Invalid("Frequency out of range".into()))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_json_shape() {
        let c: RigCommand = serde_json::from_str(r#"{"cmd":"set_freq","hz":13570000}"#).unwrap();
        assert_eq!(c, RigCommand::SetFreq { hz: 13_570_000 });
        let c: RigCommand = serde_json::from_str(r#"{"cmd":"set_mode","mode":"CWR"}"#).unwrap();
        assert_eq!(c, RigCommand::SetMode { mode: Mode::CWR });
        let c: RigCommand = serde_json::from_str(r#"{"cmd":"set_mode","mode":"AMN"}"#).unwrap();
        assert_eq!(c, RigCommand::SetMode { mode: Mode::AMN });
        assert!(serde_json::from_str::<RigCommand>(r#"{"cmd":"set_mode","mode":"PKTUSB"}"#).is_err());
        let c: RigCommand = serde_json::from_str(r#"{"cmd":"tune","hz":9500000,"mode":"AM"}"#).unwrap();
        assert_eq!(c, RigCommand::Tune { hz: 9_500_000, mode: Mode::AM });
    }

    #[test]
    fn a_mode_is_served_by_the_same_or_narrow_am() {
        assert!(Mode::AM.satisfied_by(Some("AM")));
        assert!(Mode::AM.satisfied_by(Some("AMN")));
        assert!(!Mode::AMN.satisfied_by(Some("AM")));
        assert!(!Mode::AM.satisfied_by(Some("USB")));
        assert!(!Mode::USB.satisfied_by(Some("PKTUSB")));
        assert!(!Mode::AM.satisfied_by(None));
    }

    #[test]
    fn frequency_limits() {
        let d = default_coverage();
        assert!(RigCommand::SetFreq { hz: FREQ_MIN_HZ }.validate(&d).is_ok());
        assert!(RigCommand::SetFreq { hz: FREQ_MAX_HZ }.validate(&d).is_ok());
        assert!(RigCommand::SetFreq { hz: FREQ_MIN_HZ - 1 }.validate(&d).is_err());
        assert!(RigCommand::SetFreq { hz: FREQ_MAX_HZ + 1 }.validate(&d).is_err());
        let vu = [RxRange { lo_khz: 144_000, hi_khz: 146_000 }, RxRange { lo_khz: 430_000, hi_khz: 440_000 }];
        assert!(RigCommand::SetFreq { hz: 145_500_000 }.validate(&vu).is_ok());
        assert!(RigCommand::SetFreq { hz: 14_074_000 }.validate(&vu).is_err());
        assert!(RigCommand::Tune { hz: 14_074_000, mode: Mode::USB }.validate(&vu).is_err());
        assert!(RigCommand::SetMode { mode: Mode::AM }.validate(&vu).is_ok());
    }

    fn lines(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn dump_state_ranges() {
        // rigctld 4.5 `+\dump_state` for the dummy rig (abridged after the ranges)
        let dummy = lines(&[
            "dump_state:",
            "1",
            "1",
            "0",
            "150000.000000 1500000000.000000 0x1ff -1 -1 0x17e00007 0xf",
            "0 0 0 0 0 0 0",
            "150000.000000 1500000000.000000 0x1ff 5000 100000 0x17e00007 0xf",
            "0 0 0 0 0 0 0",
            "0x1ff 1",
            "0 0",
            "RPRT 0",
        ]);
        assert_eq!(parse_dump_state(&dummy), [RxRange { lo_khz: 150, hi_khz: 1_500_000 }]);

        // an HF + VHF/UHF rig: touching ranges merge, fractional kHz round outwards
        let rig = lines(&[
            "1",
            "3073",
            "2",
            "30000 199999999 0x1bf -1 -1 0x3 0x0",
            "200000000 470000000 0x1bf -1 -1 0x3 0x0",
            "800000000 999990000.5 0x40 -1 -1 0x3 0x0",
            "0 0 0 0 0 0 0",
        ]);
        assert_eq!(
            parse_dump_state(&rig),
            [RxRange { lo_khz: 30, hi_khz: 470_000 }, RxRange { lo_khz: 800_000, hi_khz: 999_991 }]
        );
        assert!(parse_dump_state(&lines(&["RPRT -11"])).is_empty());
    }

    #[test]
    fn state_json_shape() {
        let j = serde_json::to_value(RigState::down()).unwrap();
        assert_eq!(j["link"], "down");
        assert_eq!(j["power"], "unknown");
        assert!(j["freq_hz"].is_null());
    }
}
