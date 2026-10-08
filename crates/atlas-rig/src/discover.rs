//! What the settings page offers: Hamlib models, serial ports, the rigctld version.

use std::path::{Path, PathBuf};

use atlas_core::platform::Os;
use atlas_core::setup::{parse_rig_list, RigModel, SerialPortInfo};

use crate::process::command;

/// Hamlib models from `rigctld -l` (the same list as `rigctl -l`, so installers ship one
/// binary; its startup chatter goes to stderr). Empty if rigctld is not installed.
pub async fn rig_models(rigctld: &str) -> Vec<RigModel> {
    match command(rigctld).arg("-l").output().await {
        Ok(out) => parse_rig_list(&String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            tracing::warn!("rigctld -l: {e}");
            vec![]
        }
    }
}

/// `rigctld --version` (e.g. "rigctl Hamlib 4.5.5 ..."), or `None` if it cannot run.
pub async fn rigctld_version(rigctld: &str) -> Option<String> {
    let out = command(rigctld).arg("--version").output().await.ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
}

/// May this user read and write `path`?
#[cfg(unix)]
pub fn accessible(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    // checks permissions without opening the port (opening can toggle DTR/RTS on the rig)
    unsafe { libc::access(c.as_ptr(), libc::R_OK | libc::W_OK) == 0 }
}

#[cfg(not(unix))]
pub fn accessible(_path: &Path) -> bool {
    true
}

/// The serial ports on this machine.
pub fn serial_ports() -> Vec<SerialPortInfo> {
    match Os::CURRENT {
        Os::Linux => scan_dev(Path::new("/")),
        Os::Macos => scan_cu(Path::new("/")),
        Os::Windows => com_ports(),
    }
}

/// Is there a serial port called `device`? A path on Linux and macOS; on Windows a COM
/// port the system lists (`COM5` is not a file).
pub fn port_exists(device: &str) -> bool {
    match Os::CURRENT {
        Os::Windows => {
            let name = device.strip_prefix(r"\\.\").unwrap_or(device);
            com_ports().iter().any(|p| p.path.eq_ignore_ascii_case(name))
        }
        _ => Path::new(device).exists(),
    }
}

/// Windows: COM ports with their description, e.g. "Silicon Labs Dual CP2105 USB to UART
/// Bridge: Enhanced COM Port (COM5)" (the Enhanced one is the FTDX10's CAT port).
#[cfg(windows)]
fn com_ports() -> Vec<SerialPortInfo> {
    let mut ports: Vec<SerialPortInfo> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let desc = match &p.port_type {
                serialport::SerialPortType::UsbPort(u) => u.product.clone(),
                _ => None,
            };
            let label = match desc {
                Some(d) if !d.contains(&p.port_name) => format!("{d} ({})", p.port_name),
                Some(d) => d,
                None => p.port_name.clone(),
            };
            SerialPortInfo { path: p.port_name, label, accessible: true }
        })
        .collect();
    // COM2 before COM10
    ports.sort_by_key(|p| p.path.trim_start_matches("COM").parse::<u32>().unwrap_or(u32::MAX));
    ports
}

#[cfg(not(windows))]
fn com_ports() -> Vec<SerialPortInfo> {
    vec![]
}

/// macOS: the `/dev/cu.*` call-out devices under `root`. Their `tty.*` twins wait for
/// carrier detect, and the Bluetooth / debug consoles are never a rig.
pub fn scan_cu(root: &Path) -> Vec<SerialPortInfo> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("dev"))
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.retain(|n| {
        n.starts_with("cu.") && !n.contains("Bluetooth") && !n.contains("debug-console") && !n.contains("wlan-debug")
    });
    names.sort();
    names
        .into_iter()
        .map(|n| {
            let p = root.join("dev").join(&n);
            SerialPortInfo { path: format!("/dev/{n}"), label: n, accessible: accessible(&p) }
        })
        .collect()
}

/// Linux: serial ports under `root` (`/` in production; a temp tree in tests).
///
/// Stable `/dev/serial/by-id/*` names first, then any `ttyUSB*` / `ttyACM*` they don't
/// cover. Legacy `ttyS*` ports are skipped: almost all of them are phantoms.
pub fn scan_dev(root: &Path) -> Vec<SerialPortInfo> {
    let dev = root.join("dev");
    let shown = |p: &Path| format!("/{}", p.strip_prefix(root).unwrap_or(p).to_string_lossy());
    let mut out = vec![];
    let mut covered: Vec<PathBuf> = vec![];

    let mut by_id: Vec<_> = std::fs::read_dir(dev.join("serial/by-id"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    by_id.sort();
    for link in by_id {
        let target = std::fs::canonicalize(&link).ok();
        let name = link.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let label = match &target {
            Some(t) => format!("{name} → {}", shown(t)),
            None => name,
        };
        out.push(SerialPortInfo { path: shown(&link), label, accessible: accessible(&link) });
        covered.extend(target);
    }

    let mut ttys: Vec<_> =
        std::fs::read_dir(&dev).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    ttys.retain(|p| {
        let n = p.file_name().unwrap_or_default().to_string_lossy();
        (n.starts_with("ttyUSB") || n.starts_with("ttyACM")) && !covered.contains(p)
    });
    ttys.sort();
    for p in ttys {
        out.push(SerialPortInfo { path: shown(&p), label: shown(&p), accessible: accessible(&p) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn lists_by_id_names_then_uncovered_ttys() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = std::env::temp_dir().join(format!("swatlas-ports-{}", std::process::id()));
        let dev = root.join("dev");
        std::fs::create_dir_all(dev.join("serial/by-id")).unwrap();
        for n in ["ttyUSB0", "ttyUSB1", "ttyACM0", "ttyS0", "null"] {
            std::fs::write(dev.join(n), "").unwrap();
        }
        symlink("../../ttyUSB0", dev.join("serial/by-id/usb-Silicon_Labs_CP2105-if00-port0")).unwrap();
        std::fs::set_permissions(dev.join("ttyUSB1"), std::fs::Permissions::from_mode(0o000)).unwrap();

        let ports = scan_dev(&root);
        let paths: Vec<_> = ports.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, ["/dev/serial/by-id/usb-Silicon_Labs_CP2105-if00-port0", "/dev/ttyACM0", "/dev/ttyUSB1"]);
        assert_eq!(ports[0].label, "usb-Silicon_Labs_CP2105-if00-port0 → /dev/ttyUSB0");
        assert!(ports[0].accessible);
        // root can open anything, so only check the permission flag as a normal user
        if unsafe { libc::geteuid() } != 0 {
            assert!(!ports[2].accessible);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn macos_lists_call_out_devices_only() {
        let root = std::env::temp_dir().join(format!("swatlas-cu-{}", std::process::id()));
        let dev = root.join("dev");
        std::fs::create_dir_all(&dev).unwrap();
        for n in [
            "cu.SLAB_USBtoUART",
            "tty.SLAB_USBtoUART",
            "cu.SLAB_USBtoUART3",
            "cu.Bluetooth-Incoming-Port",
            "cu.debug-console",
            "null",
        ] {
            std::fs::write(dev.join(n), "").unwrap();
        }
        let paths: Vec<_> = scan_cu(&root).into_iter().map(|p| p.path).collect();
        assert_eq!(paths, ["/dev/cu.SLAB_USBtoUART", "/dev/cu.SLAB_USBtoUART3"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn no_dev_dir_means_no_ports() {
        assert!(scan_dev(Path::new("/nonexistent-root")).is_empty());
        assert!(scan_cu(Path::new("/nonexistent-root")).is_empty());
    }

    #[tokio::test]
    async fn real_hamlib_lists_the_ftdx10() {
        let models = rig_models("rigctld").await;
        if models.is_empty() {
            eprintln!("rigctl not installed: skipping");
            return;
        }
        assert!(models.iter().any(|m| m.id == 1042 && m.model == "FTDX-10"));
        assert!(rigctld_version("rigctld").await.is_some_and(|v| v.contains("Hamlib")));
    }
}
