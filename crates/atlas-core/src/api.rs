//! The UI <-> core API. The same `Call`s travel over Tauri `invoke` (desktop) and the
//! WebSocket (remote browser), so both clients see exactly the same behaviour.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::audio::{AudioChoice, AudioHint, AudioStatus, AudioTuning};
use crate::favorites::FavoriteRequest;
use crate::geo::LatLon;
use crate::net::NetQuery;
use crate::platform::Os;
use crate::remote::{RemoteChoice, RemoteFailure, RemoteUrl, Scope};
use crate::rig::{CommandError, RigCommand, RigState};
use crate::scan::ScanRequest;
use crate::setup::{BackendKind, Hint, RigChoice, RigModel, RigctldStatus};
use crate::source::StationKey;
use crate::stations::StationQuery;
use crate::stats::StatsQuery;
use crate::update::Channel;
use crate::user_stations::UserStationDraft;

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
    /// Filtered, paged station list (result: `StationPage`).
    Search(StationQuery),
    /// Every transmitter site with what is on the air now (result: `Overview`).
    Overview,
    /// Code tables and facts about the station data (result: `CatalogMeta`).
    StationsMeta,
    SetQth(Qth),
    /// The rig's coverage and the bands and spans on its dial, for the QTH's ITU region
    /// (result: `Dial`).
    Dial,
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
    /// This build's release notes, from its section of the changelog (result: `NoteBlock[]`).
    ReleaseNotes,
    /// Check for an update now (result: `UpdateStatus`).
    CheckUpdate,
    /// Download, verify and install the offered update, then restart. Returns at once;
    /// follow it with `UpdateStatus`. Desktop window only: the WebSocket refuses it.
    InstallUpdate,
    /// Turn automatic checks on or off, pick the channel, and save both.
    SetUpdatePrefs(UpdatePrefs),
    /// Sweep a band on the dial, reading the S-meter at each step; returns at once (result:
    /// `ScanStatus`). Follow it with `ScanStatus`. Any other rig command stops it.
    StartScan(ScanRequest),
    /// Stop the scan; what it measured is kept and the rig goes back where it was.
    StopScan,
    /// The running scan's progress, or how the last one ended (result: `ScanStatus`).
    ScanStatus,
    /// Saved scans, newest first, without their samples (result: `ScanSummary[]`).
    Scans,
    /// One saved scan (result: `Scan`).
    Scan {
        id: String,
    },
    DeleteScan {
        id: String,
    },
    /// Delete every saved scan (result: the number deleted).
    DeleteScans,
    /// Every station a saved scan logged, by current station id (result: `Logged[]`).
    Logbook,
    /// Favorites by frequency, with their stations in the running schedule (result:
    /// `FavoriteView[]`).
    Favorites,
    /// Keep a frequency, or update the favorite already there (result: `Favorite`).
    AddFavorite(FavoriteRequest),
    RenameFavorite {
        id: String,
        label: String,
    },
    DeleteFavorite {
        id: String,
    },
    /// The listener's own stations by frequency, with their rows in the running catalog (result:
    /// `UserStationView[]`).
    UserStations,
    /// Keep a station of the listener's (result: `UserStationView`).
    AddUserStation(UserStationDraft),
    /// Change one; it keeps its id, so its favorites and logbook entries follow (result:
    /// `UserStationView`).
    UpdateUserStation {
        id: String,
        station: UserStationDraft,
    },
    DeleteUserStation {
        id: String,
    },
    /// Every code and known site the station form offers (result: `CodeTables`).
    CodeTables,
    /// The listener's stations as a file to keep (result: its JSON text).
    ExportUserStations,
    /// Bring stations in from an export; `replace` drops the ones kept first (result:
    /// `ImportReport`).
    ImportUserStations {
        json: String,
        replace: bool,
    },
    /// What the listening history says for a period (result: `Stats`).
    Stats(StatsQuery),
    /// The session being recorded, or the usual station at this hour (result: `ListeningNow`).
    Listening,
    /// Stations worth a try now, and those scanned but never listened to (result: `Hunt`).
    Hunt,
    /// A station's signal week by week (result: `TrendPoint[]`).
    StationTrend(StationKey),
    /// Channels listened to with a valid signal, all time (result: `FreqHeat[]`).
    ListenedFreqs,
    /// The listener chose this station as the one they hear: the session there is credited to
    /// it.
    HearingStation(StationKey),
    /// Forget the sessions that ended before `before` (Unix seconds), or all of them (result: the
    /// number forgotten).
    ClearListening {
        #[ts(type = "number | null")]
        before: Option<i64>,
    },
    /// Internet stations from the radio-browser.info directory, the NET band (result:
    /// `NetPage`). Refused while internet radio is off.
    NetSearch(NetQuery),
    /// Countries the NET band's search can filter by (result: `NetCountry[]`).
    NetCountries,
    /// Turn internet radio (the NET band) on or off, and save it.
    SetNet {
        enabled: bool,
    },
    /// Tune the receiver to an internet station instead of the rig: its audio (`GET /api/audio`,
    /// `audio_open`) is that station's stream for every listener, fetched only while someone
    /// listens. Refused while internet radio is off or if the stream can't be played (result:
    /// `NetNow`).
    NetTune {
        uuid: String,
    },
    /// Back to the rig. Tuning the rig, a scan, or turning internet radio off do it too.
    NetStop,
    /// The internet station tuned, if any (result: `NetNow | null`).
    NetNow,
    /// The internet stream's buffer: a size from `net::NET_BUFFER_STEPS_MS`, or `None` to size it
    /// automatically. Applies to the stream playing at once, and is saved.
    SetNetBuffer {
        ms: Option<u32>,
    },
    /// The internet stations kept, in memory-channel order (result: `NetFavorite[]`).
    NetFavorites,
    /// Keep an internet station, or relabel the one kept (result: `NetFavorite`).
    AddNetFavorite {
        uuid: String,
        label: Option<String>,
    },
    RenameNetFavorite {
        id: String,
        label: String,
    },
    DeleteNetFavorite {
        id: String,
    },
    /// Countries with internet stations, for the map while browsing the NET band (result:
    /// `NetDot[]`).
    NetOverview,
    /// Internet listening in a period, kept apart from the rig's Stats (result: `NetStats`).
    NetStats(StatsQuery),
    /// Remote access: is the browser UI served, where, and to how many (result: `RemoteStatus`).
    RemoteStatus,
    /// Start, restart or stop remote access, and save the choice. A failed start is in the
    /// result's `failure` (result: `RemoteStatus`). Desktop window only: the WebSocket refuses it,
    /// and swatlas-server always serves.
    SetRemote(RemoteChoice),
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
    /// The OS audio API the capture uses ("ALSA", "WASAPI", "CoreAudio").
    pub backend: String,
    /// The tuning's defaults on this OS ("Reset to defaults").
    pub defaults: AudioTuning,
}

/// Result of `Call::AudioDiagnostics`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AudioDiagnostics {
    pub status: AudioStatus,
    pub hints: Vec<AudioHint>,
}

/// Result of `Call::RemoteStatus`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RemoteStatus {
    /// swatlas-server: always serving, set only in the file.
    pub headless: bool,
    pub running: bool,
    /// What is served (while running), else what was last chosen.
    pub choice: RemoteChoice,
    pub scope: Scope,
    /// Where to open it, best first (empty while stopped).
    pub urls: Vec<RemoteUrl>,
    /// Browsers connected now.
    pub clients: u32,
    /// Why the last start failed, with the error as the OS gave it.
    pub failure: Option<RemoteFailure>,
    pub error: Option<String>,
    /// `RemoteChoice` fields forced by environment variables: a change lasts until restart.
    pub locked: Vec<String>,
    pub config_path: String,
}

/// Result of `Call::Info`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Info {
    pub version: String,
    /// `sim`, `external` or `spawn`.
    pub backend: String,
    /// The Hamlib model of the spawn backend; `None` for the others, or if `rigctl -l` fails.
    pub rig_model: Option<RigModel>,
    /// `host:port` of the external backend's `rigctld`; `None` for the others.
    pub rig_host: Option<String>,
    pub qth: Qth,
    /// UI language override (`en`, `es`); `None` = follow the browser/OS.
    pub lang: Option<String>,
    /// Rig audio is enabled: `GET /api/audio` (browser) or `audio_open` (desktop).
    pub audio: bool,
    pub audio_rate: u32,
    /// The listener's buffer (`AudioTuning::cushion_ms`, `max_ahead_ms`).
    pub audio_cushion_ms: u32,
    pub audio_max_ahead_ms: u32,
    /// A config file exists (false on a first run: the UI invites you to set up the rig).
    pub configured: bool,
    /// Where the core runs (the UI words hints, ports and devices for it).
    pub os: Os,
    /// Internet radio is on: the station list offers the NET band (`Call::NetSearch`).
    pub net: bool,
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
    use crate::country::Region;
    use crate::rig::Mode;
    use crate::stations::{Lens, Sort};

    #[test]
    fn call_json_shape() {
        let c: Call = serde_json::from_str(r#"{"cmd":"lookup","args":{"freq_hz":13570000}}"#).unwrap();
        assert_eq!(c, Call::Lookup { freq_hz: 13_570_000 });
        let c: Call = serde_json::from_str(r#"{"cmd":"search","args":{"q":"bbc"}}"#).unwrap();
        assert_eq!(c, Call::Search(StationQuery { q: "bbc".into(), ..Default::default() }));
        // an older client's `on_air` is ignored
        let c: Call = serde_json::from_str(r#"{"cmd":"search","args":{"regions":["am"],"on_air":true}}"#).unwrap();
        assert_eq!(c, Call::Search(StationQuery { regions: vec![Region::Am], ..Default::default() }));
        let c: Call = serde_json::from_str(r#"{"cmd":"overview"}"#).unwrap();
        assert_eq!(c, Call::Overview);
        let c: Call = serde_json::from_str(r#"{"cmd":"stations_meta"}"#).unwrap();
        assert_eq!(c, Call::StationsMeta);
        let c: Call =
            serde_json::from_str(r#"{"cmd":"start_scan","args":{"band_id":"49m","step_hz":null,"dwell_ms":null}}"#)
                .unwrap();
        assert_eq!(
            c,
            Call::StartScan(ScanRequest { band_id: "49m".into(), step_hz: None, dwell_ms: None, speed: None })
        );
        let c: Call = serde_json::from_str(r#"{"cmd":"delete_scan","args":{"id":"x"}}"#).unwrap();
        assert_eq!(c, Call::DeleteScan { id: "x".into() });
        let c: Call = serde_json::from_str(r#"{"cmd":"delete_scans"}"#).unwrap();
        assert_eq!(c, Call::DeleteScans);
        let c: Call = serde_json::from_str(r#"{"cmd":"search","args":{"lens":"heard","scan":"s1"}}"#).unwrap();
        assert_eq!(c, Call::Search(StationQuery { lens: Lens::Heard, scan: Some("s1".into()), ..Default::default() }));
        let c: Call = serde_json::from_str(r#"{"cmd":"search","args":{"lens":"heard","sort":"strongest"}}"#).unwrap();
        assert_eq!(c, Call::Search(StationQuery { lens: Lens::Heard, sort: Sort::Strongest, ..Default::default() }));
        let c: Call = serde_json::from_str(
            r#"{"cmd":"add_favorite","args":{"freq_hz":5905000,"mode":"AM","label":null,
                "station":{"source":"mine","key":"1-2"}}}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Call::AddFavorite(FavoriteRequest {
                freq_hz: 5_905_000,
                mode: Mode::AM,
                label: None,
                station: Some(crate::source::StationKey { source: "mine".into(), key: "1-2".into() }),
            })
        );
        for (json, call) in [
            (r#"{"cmd":"user_stations"}"#, Call::UserStations),
            (r#"{"cmd":"code_tables"}"#, Call::CodeTables),
            (r#"{"cmd":"listening"}"#, Call::Listening),
            (r#"{"cmd":"hunt"}"#, Call::Hunt),
            (r#"{"cmd":"listened_freqs"}"#, Call::ListenedFreqs),
            (
                r#"{"cmd":"stats","args":{"period":"month","tz_offset_min":-180}}"#,
                Call::Stats(StatsQuery { period: crate::stats::Period::Month, tz_offset_min: -180 }),
            ),
            (
                r#"{"cmd":"stats","args":{"period":"all"}}"#,
                Call::Stats(StatsQuery { period: crate::stats::Period::All, tz_offset_min: 0 }),
            ),
            (
                r#"{"cmd":"hearing_station","args":{"source":"eibi","key":"k"}}"#,
                Call::HearingStation(StationKey { source: "eibi".into(), key: "k".into() }),
            ),
            (
                r#"{"cmd":"station_trend","args":{"source":"mine","key":"1"}}"#,
                Call::StationTrend(StationKey { source: "mine".into(), key: "1".into() }),
            ),
            (r#"{"cmd":"clear_listening","args":{"before":null}}"#, Call::ClearListening { before: None }),
            (r#"{"cmd":"clear_listening","args":{"before":5}}"#, Call::ClearListening { before: Some(5) }),
            (r#"{"cmd":"export_user_stations"}"#, Call::ExportUserStations),
            (
                r#"{"cmd":"net_search","args":{"q":"rri","country":"ROU","offset":0,"limit":50}}"#,
                Call::NetSearch(NetQuery { q: "rri".into(), country: Some("ROU".into()), offset: 0, limit: 50 }),
            ),
            (r#"{"cmd":"net_search","args":{}}"#, Call::NetSearch(NetQuery::default())),
            (r#"{"cmd":"set_net","args":{"enabled":true}}"#, Call::SetNet { enabled: true }),
            (r#"{"cmd":"net_countries"}"#, Call::NetCountries),
            (r#"{"cmd":"net_tune","args":{"uuid":"u1"}}"#, Call::NetTune { uuid: "u1".into() }),
            (r#"{"cmd":"net_favorites"}"#, Call::NetFavorites),
            (
                r#"{"cmd":"add_net_favorite","args":{"uuid":"u1","label":null}}"#,
                Call::AddNetFavorite { uuid: "u1".into(), label: None },
            ),
            (
                r#"{"cmd":"rename_net_favorite","args":{"id":"1-u1","label":"Mitre"}}"#,
                Call::RenameNetFavorite { id: "1-u1".into(), label: "Mitre".into() },
            ),
            (r#"{"cmd":"delete_net_favorite","args":{"id":"1-u1"}}"#, Call::DeleteNetFavorite { id: "1-u1".into() }),
            (r#"{"cmd":"net_overview"}"#, Call::NetOverview),
            (r#"{"cmd":"net_stop"}"#, Call::NetStop),
            (r#"{"cmd":"net_now"}"#, Call::NetNow),
            (r#"{"cmd":"set_net_buffer","args":{"ms":null}}"#, Call::SetNetBuffer { ms: None }),
            (r#"{"cmd":"set_net_buffer","args":{"ms":4000}}"#, Call::SetNetBuffer { ms: Some(4000) }),
            (
                r#"{"cmd":"net_stats","args":{"period":"week","tz_offset_min":60}}"#,
                Call::NetStats(StatsQuery { period: crate::stats::Period::Week, tz_offset_min: 60 }),
            ),
            (r#"{"cmd":"delete_user_station","args":{"id":"x"}}"#, Call::DeleteUserStation { id: "x".into() }),
            (
                r#"{"cmd":"import_user_stations","args":{"json":"{}","replace":true}}"#,
                Call::ImportUserStations { json: "{}".into(), replace: true },
            ),
        ] {
            assert_eq!(serde_json::from_str::<Call>(json).unwrap(), call, "{json}");
        }
        let draft = r#"{"freq_hz":6075000,"mode":"USB","name":"Radio Local","itu":"CHL",
            "site":{"kind":"country","itu":"CHL"},"notes":"","url":null,
            "slots":[{"start":0,"end":1440,"days":{"t":"daily"},"lang":"S","target":""}]}"#;
        let c: Call = serde_json::from_str(&format!(r#"{{"cmd":"add_user_station","args":{draft}}}"#)).unwrap();
        let Call::AddUserStation(d) = &c else { panic!("{c:?}") };
        assert_eq!((d.name.as_str(), d.slots.len()), ("Radio Local", 1));
        let c: Call = serde_json::from_str(&format!(
            r#"{{"cmd":"update_user_station","args":{{"id":"1-2","station":{draft}}}}}"#
        ))
        .unwrap();
        assert!(matches!(c, Call::UpdateUserStation { id, .. } if id == "1-2"));
        let c: Call = serde_json::from_str(r#"{"cmd":"rename_favorite","args":{"id":"x","label":"y"}}"#).unwrap();
        assert_eq!(c, Call::RenameFavorite { id: "x".into(), label: "y".into() });
        let c: Call = serde_json::from_str(r#"{"cmd":"favorites"}"#).unwrap();
        assert_eq!(c, Call::Favorites);
        let c: Call = serde_json::from_str(r#"{"cmd":"rig","args":{"cmd":"set_power","on":false}}"#).unwrap();
        assert_eq!(c, Call::Rig(RigCommand::SetPower { on: false }));
        let c: Call = serde_json::from_str(
            r#"{"cmd":"apply_audio","args":{"enabled":true,"device":"default","rate":16000,"tuning":{
                "block_ms":20,"queue_ms":1000,"capture_rate":48000,"cushion_ms":120,"max_ahead_ms":500}}}"#,
        )
        .unwrap();
        let tuning = crate::audio::default_tuning(Os::Linux);
        assert_eq!(c, Call::ApplyAudio(AudioChoice { enabled: true, device: "default".into(), rate: 16_000, tuning }));
        // a page from before the ffmpeg capture was replaced still sends its ffmpeg fields
        let old: Call = serde_json::from_str(
            r#"{"cmd":"apply_audio","args":{"enabled":true,"device":"default","rate":16000,"tuning":{
                "block_ms":20,"queue_ms":1000,"input_queue":0,"drift_correction":0,"capture_buffer_ms":50,
                "capture_rate":48000,"cushion_ms":120,"max_ahead_ms":500}}}"#,
        )
        .unwrap();
        assert_eq!(old, c);
    }

    #[test]
    fn error_kinds_follow_command_errors() {
        let e: ApiError = CommandError::Conflict("The radio is off".into()).into();
        assert_eq!(e.kind, ErrorKind::Conflict);
        assert_eq!(e.message, "The radio is off");
    }
}
