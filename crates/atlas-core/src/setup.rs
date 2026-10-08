//! Rig setup: the connection config, what the settings page may change, Hamlib model list
//! parsing, choice validation, and plain-language diagnosis of connection problems.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::platform::Os;
use crate::rig::{Link, Power, RigState};

/// CAT baud rates offered in the UI (0 = Hamlib's default for the model).
pub const BAUD_RATES: [u32; 6] = [4800, 9600, 19200, 38400, 57600, 115200];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum BackendKind {
    /// Simulated rig, no hardware.
    #[default]
    Sim,
    /// Connect to a `rigctld` that is already running.
    External,
    /// Start and supervise our own `rigctld`.
    Spawn,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sim => "sim",
            Self::External => "external",
            Self::Spawn => "spawn",
        }
    }
}

/// `[rig]` in `swatlas.toml`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RigConfig {
    pub backend: BackendKind,
    pub host: String,
    pub port: u16,
    /// Hamlib model number (`rigctl -l`), e.g. 1042 for the FTDX10. Spawn only.
    pub model: u32,
    /// Serial device, e.g. `/dev/serial/by-id/...-if00-port0`. Spawn only.
    pub device: String,
    /// CAT baud rate; 0 = Hamlib's default. Spawn only.
    pub baud: u32,
    /// `rigctld` executable. Spawn only. Never settable from the UI (see `RigChoice`).
    pub rigctld: String,
}

impl Default for RigConfig {
    fn default() -> Self {
        Self {
            backend: BackendKind::Sim,
            host: "127.0.0.1".into(),
            port: 4532,
            model: 1,
            device: String::new(),
            baud: 0,
            rigctld: "rigctld".into(),
        }
    }
}

/// The part of `RigConfig` the settings page may change. It deliberately has no
/// executable path, so no client can make the core run an arbitrary program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RigChoice {
    pub backend: BackendKind,
    pub host: String,
    pub port: u16,
    pub model: u32,
    pub device: String,
    pub baud: u32,
}

impl RigConfig {
    pub fn choice(&self) -> RigChoice {
        RigChoice {
            backend: self.backend,
            host: self.host.clone(),
            port: self.port,
            model: self.model,
            device: self.device.clone(),
            baud: self.baud,
        }
    }

    pub fn with_choice(&self, c: &RigChoice) -> Self {
        Self {
            backend: c.backend,
            host: c.host.clone(),
            port: c.port,
            model: c.model,
            device: c.device.clone(),
            baud: c.baud,
            rigctld: self.rigctld.clone(),
        }
    }
}

/// One Hamlib model (`rigctl -l`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RigModel {
    pub id: u32,
    pub mfg: String,
    pub model: String,
    /// Hamlib backend status: Stable, Beta, Alpha, Untested...
    pub status: String,
}

/// Parse `rigctl -l`. Columns are fixed-width (a long name can fill its column with no
/// gap before the next), so rows are sliced at the header's column offsets.
///
/// Hamlib's own pseudo-rigs are dropped, except Dummy (model 1) for testing.
pub fn parse_rig_list(text: &str) -> Vec<RigModel> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return vec![] };
    let col = |name: &str| header.find(name);
    let (Some(mfg), Some(model), Some(version), Some(status), Some(mac)) =
        (col("Mfg"), col("Model"), col("Version"), col("Status"), col("Macro"))
    else {
        return vec![];
    };
    let field = |l: &str, a: usize, b: usize| l.get(a..b.min(l.len())).unwrap_or("").trim().to_string();
    lines
        .filter_map(|l| {
            let id = l.get(..mfg)?.trim().parse::<u32>().ok()?;
            Some(RigModel {
                id,
                mfg: field(l, mfg, model),
                model: field(l, model, version),
                status: field(l, status, mac),
            })
        })
        .filter(|m| m.mfg != "Hamlib" || m.id == 1)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SerialPortInfo {
    /// Path to use in the config (the stable `/dev/serial/by-id/...` one when there is one).
    pub path: String,
    /// What to show: the by-id name and the tty it points to.
    pub label: String,
    /// Whether this user may open it (false usually means: not in the `dialout` group).
    pub accessible: bool,
}

/// Why the serial device cannot be used, checked before starting rigctld.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DeviceProblem {
    Missing,
    PermissionDenied,
}

/// State of the `rigctld` we start (spawn backend).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RigctldStatus {
    pub running: bool,
    /// Seconds since the current process started.
    pub uptime_s: Option<u32>,
    pub restarts: u32,
    pub last_exit: Option<String>,
    /// The executable could not be started at all.
    pub spawn_error: Option<String>,
    /// Something else already listens on the rigctld TCP port.
    pub port_in_use: bool,
    /// The serial device is missing or not accessible (rigctld is not started then).
    pub device_problem: Option<DeviceProblem>,
    /// Last lines rigctld printed (stderr).
    pub log: Vec<String>,
}

/// A likely cause of a connection problem, explained in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Hint {
    /// rigctld is not installed (or `rig.rigctld` points nowhere).
    RigctldMissing,
    /// The serial port exists but this user may not open it (`dialout` group).
    PermissionDenied,
    /// The serial port does not exist (rig unplugged, or wrong port).
    DeviceMissing,
    /// Another program has the serial port open.
    DeviceBusy,
    /// Another program (often another rigctld) listens on the rigctld TCP port.
    TcpPortInUse,
    /// rigctld runs but the radio does not answer (baud vs CAT RATE, cable, radio off).
    NoAnswer,
    /// No rigctld at the configured host/port (external backend).
    RigctldUnreachable,
}

/// rigctld keeps trying to open a silent rig for ~15 s before it even listens.
const NO_ANSWER_AFTER_S: u32 = 20;

/// Turn what we can observe into hints, most specific first.
pub fn diagnose(os: Os, backend: BackendKind, rigctld: Option<&RigctldStatus>, state: &RigState) -> Vec<Hint> {
    let mut out = vec![];
    if let Some(st) = rigctld {
        if st.spawn_error.is_some() {
            out.push(Hint::RigctldMissing);
        }
        if st.port_in_use {
            out.push(Hint::TcpPortInUse);
        }
        match st.device_problem {
            Some(DeviceProblem::Missing) => out.push(Hint::DeviceMissing),
            Some(DeviceProblem::PermissionDenied) => out.push(Hint::PermissionDenied),
            None => {}
        }
        // rigctld's own words, when it prints them (it often doesn't), e.g.
        // "serial_open: Unable to open /dev/ttyUSB0 - Permission denied"
        if let Some(l) = st.log.iter().rev().find(|l| l.contains("Unable to open")).filter(|_| out.is_empty()) {
            out.extend(open_error_hint(os, l));
        }
    }
    if !out.is_empty() {
        return out;
    }
    let waited = rigctld.and_then(|s| s.uptime_s).unwrap_or(0) >= NO_ANSWER_AFTER_S;
    match (backend, state.link, state.power) {
        (BackendKind::External, Link::Down, _) => out.push(Hint::RigctldUnreachable),
        (BackendKind::Spawn, Link::Down, _) if waited => out.push(Hint::NoAnswer),
        (BackendKind::Spawn | BackendKind::External, Link::Ok, Power::Unknown) => out.push(Hint::NoAnswer),
        _ => {}
    }
    out
}

/// What a failed serial open means. On Windows there are no device permissions: Hamlib's
/// serial layer reports a COM port held by another program as "Permission denied"
/// (`ERROR_ACCESS_DENIED`), so there it means busy.
fn open_error_hint(os: Os, line: &str) -> Option<Hint> {
    let denied = line.contains("Permission denied") || line.contains("Access is denied");
    if denied {
        Some(if os == Os::Windows { Hint::DeviceBusy } else { Hint::PermissionDenied })
    } else if line.contains("No such file or directory") || line.contains("cannot find the file") {
        Some(Hint::DeviceMissing)
    } else if line.contains("Device or resource busy") || line.contains("Resource busy") {
        Some(Hint::DeviceBusy)
    } else {
        None
    }
}

/// Check a choice from the UI before applying it. `device_exists` is injected for tests.
pub fn validate_choice(
    os: Os,
    c: &RigChoice,
    models: &[RigModel],
    device_exists: impl Fn(&str) -> bool,
) -> Result<(), String> {
    if c.port < 1024 {
        return Err("The rigctld port must be between 1024 and 65535".into());
    }
    match c.backend {
        BackendKind::Sim => Ok(()),
        BackendKind::External => {
            if c.host.trim().is_empty() || c.host.chars().any(char::is_whitespace) {
                return Err("Enter the rigctld host".into());
            }
            Ok(())
        }
        BackendKind::Spawn => {
            if c.model == 0 || (!models.is_empty() && !models.iter().any(|m| m.id == c.model)) {
                return Err(format!("Unknown Hamlib model {}", c.model));
            }
            if c.baud != 0 && !BAUD_RATES.contains(&c.baud) {
                return Err(format!("Unsupported baud rate {}", c.baud));
            }
            // Hamlib's Dummy rig needs no port; every real one does.
            if c.device.is_empty() && c.model != 1 {
                return Err("Choose the serial port".into());
            }
            if !c.device.is_empty() && (!valid_device_path(os, &c.device) || !device_exists(&c.device)) {
                return Err(format!("No serial port at {}", c.device));
            }
            Ok(())
        }
    }
}

/// A serial port name this OS uses: `/dev/...` (Linux), `/dev/cu.*` or `/dev/tty.*`
/// (macOS), `COM5` or `\\.\COM12` (Windows).
pub fn valid_device_path(os: Os, p: &str) -> bool {
    let unix_ok = !p.contains("..") && !p.contains('\0');
    match os {
        Os::Linux => p.starts_with("/dev/") && unix_ok,
        Os::Macos => (p.starts_with("/dev/cu.") || p.starts_with("/dev/tty.")) && unix_ok,
        Os::Windows => {
            let n = p.strip_prefix(r"\\.\").unwrap_or(p);
            n.strip_prefix("COM").is_some_and(|d| (1..=3).contains(&d.len()) && d.chars().all(|c| c.is_ascii_digit()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real Hamlib 4.5.5 output, including rows whose names fill their columns.
    const RIG_LIST: &str = " Rig #  Mfg                    Model                   Version         Status      Macro
     1  Hamlib                 Dummy                   20221128.0      Stable      RIG_MODEL_DUMMY
     2  Hamlib                 NET rigctl              20230328.0      Stable      RIG_MODEL_NETRIGCTL
  1042  Yaesu                  FTDX-10                 20230328.6      Stable      RIG_MODEL_FTDX10
  1018  Yaesu                  FRG-9600                20160409.0      Alpha       RIG_MODEL_FRG9600
 23003  DTTS Microwave Society DttSP IPC               20200319.0      Beta        RIG_MODEL_DTTSP
 25003  Coding Technologies    Digital World Traveller 20200112.0      Alpha       RIG_MODEL_DWT
";

    fn models() -> Vec<RigModel> {
        parse_rig_list(RIG_LIST)
    }

    #[test]
    fn parses_fixed_width_rows() {
        let m = models();
        assert_eq!(m.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 1042, 1018, 23003, 25003]);
        let ftdx10 = &m[1];
        assert_eq!(
            (ftdx10.mfg.as_str(), ftdx10.model.as_str(), ftdx10.status.as_str()),
            ("Yaesu", "FTDX-10", "Stable")
        );
        assert_eq!(m[3].mfg, "DTTS Microwave Society");
        assert_eq!(m[4].model, "Digital World Traveller");
    }

    #[test]
    fn garbage_gives_no_models() {
        assert!(parse_rig_list("").is_empty());
        assert!(parse_rig_list("rigctl: command not found").is_empty());
    }

    fn spawn(model: u32, device: &str, baud: u32) -> RigChoice {
        RigChoice {
            backend: BackendKind::Spawn,
            host: "127.0.0.1".into(),
            port: 4532,
            model,
            device: device.into(),
            baud,
        }
    }

    #[test]
    fn validates_spawn_choices() {
        let m = models();
        let exists = |p: &str| p == "/dev/ttyUSB0";
        assert!(validate_choice(Os::Linux, &spawn(1042, "/dev/ttyUSB0", 9600), &m, exists).is_ok());
        assert!(validate_choice(Os::Linux, &spawn(1, "", 0), &m, exists).is_ok());
        assert!(validate_choice(Os::Linux, &spawn(1042, "", 9600), &m, exists).is_err());
        assert!(validate_choice(Os::Linux, &spawn(9999, "/dev/ttyUSB0", 9600), &m, exists).is_err());
        assert!(validate_choice(Os::Linux, &spawn(1042, "/dev/ttyUSB0", 1234), &m, exists).is_err());
        assert!(validate_choice(Os::Linux, &spawn(1042, "/dev/ttyUSB9", 9600), &m, exists).is_err());
        assert!(validate_choice(Os::Linux, &spawn(1042, "/etc/passwd", 9600), &m, |_| true).is_err());
        assert!(validate_choice(Os::Linux, &spawn(1042, "/dev/../etc/passwd", 9600), &m, |_| true).is_err());
        // without a model list (rigctl missing) any model id is accepted
        assert!(validate_choice(Os::Linux, &spawn(9999, "/dev/ttyUSB0", 9600), &[], exists).is_ok());
    }

    #[test]
    fn validates_external_and_ports() {
        let mut c = spawn(1, "", 0);
        c.backend = BackendKind::External;
        assert!(validate_choice(Os::Linux, &c, &[], |_| false).is_ok());
        c.host = " ".into();
        assert!(validate_choice(Os::Linux, &c, &[], |_| false).is_err());
        c.host = "127.0.0.1".into();
        c.port = 80;
        assert!(validate_choice(Os::Linux, &c, &[], |_| false).is_err());
    }

    #[test]
    fn choice_never_changes_the_executable() {
        let cfg = RigConfig { rigctld: "/opt/hamlib/bin/rigctld".into(), ..RigConfig::default() };
        let next = cfg.with_choice(&spawn(1042, "/dev/ttyUSB0", 9600));
        assert_eq!(next.rigctld, "/opt/hamlib/bin/rigctld");
        assert_eq!(next.choice(), spawn(1042, "/dev/ttyUSB0", 9600));
    }

    fn status(log: &[&str]) -> RigctldStatus {
        RigctldStatus {
            running: true,
            uptime_s: Some(3),
            log: log.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn diagnoses_serial_errors_from_the_rigctld_log() {
        let down = RigState::down();
        // the hamlib_settings line is noise and must not be taken for a missing device
        let missing = status(&[
            "rig_settings_load_all: settings_file (/home/u/.config/hamlib_settings): No such file or directory",
            "serial_open: Unable to open /dev/serial/by-id/nope - No such file or directory",
        ]);
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&missing), &down), [Hint::DeviceMissing]);
        let denied = status(&["serial_open: Unable to open /dev/ttyUSB0 - Permission denied"]);
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&denied), &down), [Hint::PermissionDenied]);
        let busy = status(&["serial_open: Unable to open /dev/ttyUSB0 - Device or resource busy"]);
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&busy), &down), [Hint::DeviceBusy]);
        let noise = status(&["rig_settings_load_all: settings_file (x): No such file or directory"]);
        assert!(diagnose(Os::Linux, BackendKind::Spawn, Some(&noise), &down).is_empty());
    }

    #[test]
    fn serial_errors_mean_different_things_per_os() {
        let down = RigState::down();
        // Hamlib on Windows: a COM port held by another program
        let held = status(&["serial_open: Unable to open COM5 - Permission denied"]);
        assert_eq!(diagnose(Os::Windows, BackendKind::Spawn, Some(&held), &down), [Hint::DeviceBusy]);
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&held), &down), [Hint::PermissionDenied]);
        let gone = status(&["serial_open: Unable to open COM9 - No such file or directory"]);
        assert_eq!(diagnose(Os::Windows, BackendKind::Spawn, Some(&gone), &down), [Hint::DeviceMissing]);
        let mac_busy = status(&["serial_open: Unable to open /dev/cu.SLAB_USBtoUART - Resource busy"]);
        assert_eq!(diagnose(Os::Macos, BackendKind::Spawn, Some(&mac_busy), &down), [Hint::DeviceBusy]);
    }

    #[test]
    fn device_paths_per_os() {
        for p in ["/dev/ttyUSB0", "/dev/serial/by-id/usb-Silicon_Labs_CP2105-if00-port0"] {
            assert!(valid_device_path(Os::Linux, p), "{p}");
        }
        for p in ["COM5", "/dev/../etc/passwd", "/etc/passwd"] {
            assert!(!valid_device_path(Os::Linux, p), "{p}");
        }
        for p in ["/dev/cu.SLAB_USBtoUART", "/dev/cu.usbserial-01A82072", "/dev/tty.SLAB_USBtoUART3"] {
            assert!(valid_device_path(Os::Macos, p), "{p}");
        }
        for p in ["/dev/ttyUSB0", "/dev/cu.../x"] {
            assert!(!valid_device_path(Os::Macos, p), "{p}");
        }
        for p in ["COM1", "COM5", "COM123", r"\\.\COM12"] {
            assert!(valid_device_path(Os::Windows, p), "{p}");
        }
        for p in ["COM", "COM1234", "COMx", "com5", r"C:\Windows", "/dev/ttyUSB0", "LPT1"] {
            assert!(!valid_device_path(Os::Windows, p), "{p}");
        }
    }

    #[test]
    fn diagnoses_process_problems() {
        let down = RigState::down();
        let missing = RigctldStatus { spawn_error: Some("No such file or directory".into()), ..Default::default() };
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&missing), &down), [Hint::RigctldMissing]);
        let taken = RigctldStatus { port_in_use: true, ..Default::default() };
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&taken), &down), [Hint::TcpPortInUse]);
        let gone = RigctldStatus { device_problem: Some(DeviceProblem::Missing), ..Default::default() };
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&gone), &down), [Hint::DeviceMissing]);
        let denied = RigctldStatus { device_problem: Some(DeviceProblem::PermissionDenied), ..Default::default() };
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&denied), &down), [Hint::PermissionDenied]);
    }

    #[test]
    fn diagnoses_a_silent_radio() {
        // rigctld still opening the rig: give it time before blaming the radio
        let young = status(&[]);
        assert!(diagnose(Os::Linux, BackendKind::Spawn, Some(&young), &RigState::down()).is_empty());
        let old = RigctldStatus { uptime_s: Some(25), ..young };
        assert_eq!(diagnose(Os::Linux, BackendKind::Spawn, Some(&old), &RigState::down()), [Hint::NoAnswer]);
        // listening, but powerstat times out (RPRT -5)
        let unknown = RigState::without_rig(Link::Ok, Power::Unknown);
        assert_eq!(diagnose(Os::Linux, BackendKind::External, None, &unknown), [Hint::NoAnswer]);
    }

    #[test]
    fn diagnoses_external_and_healthy_links() {
        assert_eq!(diagnose(Os::Linux, BackendKind::External, None, &RigState::down()), [Hint::RigctldUnreachable]);
        let off = RigState::without_rig(Link::Ok, Power::Off);
        assert!(diagnose(Os::Linux, BackendKind::External, None, &off).is_empty());
        assert!(diagnose(Os::Linux, BackendKind::Sim, None, &RigState::without_rig(Link::Sim, Power::On)).is_empty());
    }
}
