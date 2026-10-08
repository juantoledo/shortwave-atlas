//! The UI <-> core API. The same `Call`s travel over Tauri `invoke` (desktop) and the
//! WebSocket (remote browser), so both clients see exactly the same behaviour.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::audio::{AudioChoice, AudioHint, AudioStatus};
use crate::geo::LatLon;
use crate::platform::Os;
use crate::rig::{CommandError, RigCommand, RigState};
use crate::setup::{BackendKind, Hint, RigChoice, RigctldStatus};
use crate::update::Channel;

/// The listener's location.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Qth {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

impl Qth {
    pub fn pos(&self) -> LatLon {
        LatLon { lat: self.lat, lon: self.lon }
    }
}

impl Default for Qth {
    fn default() -> Self {
        Self { name: "Santiago, Chile".into(), lat: -33.45, lon: -70.67 }
    }
}

/// One request from the UI. JSON: `{"cmd": "lookup", "args": {"freq_hz": 13570000}}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "cmd", content = "args", rename_all = "snake_case")]
#[ts(export)]
pub enum Call {
    /// App info and settings the UI needs at startup.
    Info,
    /// Current rig state (later changes arrive as events).
    State,
    Rig(RigCommand),
    /// Stations near a frequency, best first (result: `Candidate[]`).
    Lookup {
        freq_hz: u32,
    },
    /// Every station by frequency (result: `Candidate[]`).
    List,
    SetQth(Qth),
    /// Current rig choice and what the page needs around it (result: `RigSettings`).
    RigSettings,
    /// Hamlib models (result: `RigModel[]`, empty if `rigctl` is missing).
    RigModels,
    /// Serial ports on this machine (result: `SerialPortInfo[]`).
    SerialPorts,
    /// Validate, switch to, and save a rig choice.
    ApplyRig(RigChoice),
    /// Stop talking to the rig until the next apply or restart.
    DisconnectRig,
    /// Live connection diagnostics (result: `RigDiagnostics`).
    RigDiagnostics,
    /// Current audio choice and what the page needs around it (result: `AudioSettings`).
    AudioSettings,
    /// Capture-capable sound cards on this machine (result: `SoundCard[]`).
    SoundCards,
    /// Validate, switch the capture over, and save an audio choice.
    ApplyAudio(AudioChoice),
    /// Live capture status (result: `AudioDiagnostics`).
    AudioDiagnostics,
    /// Update check and install progress (result: `UpdateStatus`).
    UpdateStatus,
    /// Check for an update now (result: `UpdateStatus`).
    CheckUpdate,
    /// Download, verify and install the offered update, then restart. Returns at once;
    /// follow it with `UpdateStatus`. Desktop window only: the WebSocket refuses it.
    InstallUpdate,
    /// Turn automatic checks on or off, pick the channel, and save both.
    SetUpdatePrefs(UpdatePrefs),
}

/// Argument of `Call::SetUpdatePrefs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdatePrefs {
    pub check: bool,
    pub channel: Channel,
}

/// Result of `Call::RigSettings`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RigSettings {
    pub choice: RigChoice,
    /// `RigChoice` fields forced by environment variables: a change lasts until restart.
    pub locked: Vec<String>,
    pub config_path: String,
    /// `rigctld --version`, or `None` if it is not installed.
    pub rigctld_version: Option<String>,
}

/// Result of `Call::RigDiagnostics`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RigDiagnostics {
    pub backend: BackendKind,
    /// Only for the spawn backend.
    pub rigctld: Option<RigctldStatus>,
    /// Last error talking to rigctld (e.g. `RPRT -5`, connection refused).
    pub last_error: Option<String>,
    pub hints: Vec<Hint>,
}

/// Result of `Call::AudioSettings`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioSettings {
    pub choice: AudioChoice,
    /// `AudioChoice` fields forced by environment variables: a change lasts until restart.
    pub locked: Vec<String>,
    pub config_path: String,
    /// `ffmpeg -version`, or `None` if it is not installed.
    pub ffmpeg_version: Option<String>,
}

/// Result of `Call::AudioDiagnostics`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioDiagnostics {
    pub status: AudioStatus,
    pub hints: Vec<AudioHint>,
}

/// Result of `Call::Info`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Info {
    pub version: String,
    /// `sim`, `external` or `spawn`.
    pub backend: String,
    pub qth: Qth,
    /// UI language override (`en`, `es`); `None` = follow the browser/OS.
    pub lang: Option<String>,
    /// Rig audio is enabled: `GET /api/audio` (browser) or `audio_open` (desktop).
    pub audio: bool,
    pub audio_rate: u32,
    /// A config file exists (false on a first run: the UI invites you to set up the rig).
    pub configured: bool,
    /// Where the core runs (the UI words hints, ports and devices for it).
    pub os: Os,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ErrorKind {
    Invalid,
    Conflict,
    Rig,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiError {
    pub kind: ErrorKind,
    pub message: String,
}

impl ApiError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self { kind: ErrorKind::Internal, message: message.into() }
    }
}

impl From<CommandError> for ApiError {
    fn from(e: CommandError) -> Self {
        let kind = match e {
            CommandError::Invalid(_) => ErrorKind::Invalid,
            CommandError::Conflict(_) => ErrorKind::Conflict,
            CommandError::Rig(_) => ErrorKind::Rig,
        };
        Self { kind, message: e.to_string() }
    }
}

/// WebSocket client -> server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClientMsg {
    pub id: u32,
    pub call: Call,
}

/// WebSocket server -> client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum ServerMsg {
    /// Pushed on connect and on every change.
    State { state: RigState },
    Reply {
        id: u32,
        #[ts(type = "unknown")]
        ok: Option<serde_json::Value>,
        err: Option<ApiError>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_json_shape() {
        let c: Call = serde_json::from_str(r#"{"cmd":"lookup","args":{"freq_hz":13570000}}"#).unwrap();
        assert_eq!(c, Call::Lookup { freq_hz: 13_570_000 });
        let c: Call = serde_json::from_str(r#"{"cmd":"list"}"#).unwrap();
        assert_eq!(c, Call::List);
        let c: Call = serde_json::from_str(r#"{"cmd":"rig","args":{"cmd":"set_power","on":false}}"#).unwrap();
        assert_eq!(c, Call::Rig(RigCommand::SetPower { on: false }));
        let c: Call =
            serde_json::from_str(r#"{"cmd":"apply_audio","args":{"enabled":true,"device":"default","rate":16000}}"#)
                .unwrap();
        assert_eq!(c, Call::ApplyAudio(AudioChoice { enabled: true, device: "default".into(), rate: 16_000 }));
    }

    #[test]
    fn error_kinds_follow_command_errors() {
        let e: ApiError = CommandError::Conflict("The radio is off".into()).into();
        assert_eq!(e.kind, ErrorKind::Conflict);
        assert_eq!(e.message, "The radio is off");
    }
}
