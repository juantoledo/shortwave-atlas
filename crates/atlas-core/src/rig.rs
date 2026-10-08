//! Rig state and commands shared by the rig backends, the Tauri shell and the WebSocket.
//!
//! Receive only: there is deliberately no PTT or anything else that transmits.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tuning limits (FTDX10 general coverage receive range).
pub const FREQ_MIN_HZ: u32 = 30_000;
pub const FREQ_MAX_HZ: u32 = 56_000_000;

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
            Self::FM => "FM",
        }
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
    /// Reject out-of-range input before anything reaches the rig.
    pub fn validate(&self) -> Result<(), CommandError> {
        match *self {
            Self::SetFreq { hz } if !(FREQ_MIN_HZ..=FREQ_MAX_HZ).contains(&hz) => {
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
        assert!(serde_json::from_str::<RigCommand>(r#"{"cmd":"set_mode","mode":"PKTUSB"}"#).is_err());
    }

    #[test]
    fn frequency_limits() {
        assert!(RigCommand::SetFreq { hz: FREQ_MIN_HZ }.validate().is_ok());
        assert!(RigCommand::SetFreq { hz: FREQ_MAX_HZ }.validate().is_ok());
        assert!(RigCommand::SetFreq { hz: FREQ_MIN_HZ - 1 }.validate().is_err());
        assert!(RigCommand::SetFreq { hz: FREQ_MAX_HZ + 1 }.validate().is_err());
    }

    #[test]
    fn state_json_shape() {
        let j = serde_json::to_value(RigState::down()).unwrap();
        assert_eq!(j["link"], "down");
        assert_eq!(j["power"], "unknown");
        assert!(j["freq_hz"].is_null());
    }
}
