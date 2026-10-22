//! Remote access: the browser UI served by the desktop app (Settings → Remote access) or by
//! swatlas-server. Which addresses to offer, what a failed start means on each OS, and the QR
//! code of an address. There is no password: whoever reaches the port can use the app, so the
//! page warns before it listens beyond this computer.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr};

use qrcode::{Color, EcLevel, QrCode};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::platform::Os;

/// Bind address of "My network and Tailscale".
pub const NETWORK_BIND: &str = "0.0.0.0";
/// Bind address of "This computer only".
pub const LOOPBACK_BIND: &str = "127.0.0.1";

/// The `[server]` table, as the Remote access page sets it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RemoteChoice {
    pub enabled: bool,
    pub bind: String,
    pub port: u16,
}

/// Who can connect, from the bind address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Scope {
    /// This computer only.
    Loopback,
    /// Every network the computer is on: the LAN and Tailscale.
    Network,
    /// One address set in the file (a Tailscale IP, say).
    Custom,
}

pub fn scope_of(bind: &str) -> Scope {
    if bind == "localhost" {
        return Scope::Loopback;
    }
    match bind.parse::<IpAddr>() {
        Ok(ip) if ip.is_loopback() => Scope::Loopback,
        Ok(ip) if ip.is_unspecified() => Scope::Network,
        _ => Scope::Custom,
    }
}

/// Refuse settings nothing could listen on.
pub fn validate_remote(c: &RemoteChoice) -> Result<(), String> {
    if c.port == 0 {
        return Err("server.port must not be 0".into());
    }
    if c.bind.trim().is_empty() {
        return Err("server.bind must be an address".into());
    }
    Ok(())
}

/// Why the server could not start listening (the page words it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RemoteFailure {
    /// Another program listens on the port.
    PortInUse,
    /// Windows: the port is in a range Windows keeps for itself (Hyper-V, WSL).
    PortReserved,
    /// Linux and macOS: ports below 1024 need privileges.
    PortPrivileged,
    /// The bind address is not one of this computer's.
    NoSuchAddress,
    /// The settings are refused (`validate_remote`).
    Invalid,
    Other,
}

/// What a failed bind means on `os`.
pub fn bind_failure(os: Os, kind: ErrorKind) -> RemoteFailure {
    match kind {
        ErrorKind::AddrInUse => RemoteFailure::PortInUse,
        // WSAEACCES: an excluded port range (`netsh int ipv4 show excludedportrange`)
        ErrorKind::PermissionDenied if os == Os::Windows => RemoteFailure::PortReserved,
        ErrorKind::PermissionDenied => RemoteFailure::PortPrivileged,
        ErrorKind::AddrNotAvailable => RemoteFailure::NoSuchAddress,
        _ => RemoteFailure::Other,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum UrlKind {
    Tailscale,
    Lan,
    Loopback,
}

/// An address to open the UI at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RemoteUrl {
    pub url: String,
    pub kind: UrlKind,
    /// What the QR code opens: the phone remote at that address (`PHONE_PAGE`).
    pub qr_url: String,
    pub qr: Qr,
}

/// The UI's page for phones, next to the atlas's `index.html`: the QR codes open it.
pub const PHONE_PAGE: &str = "remote.html";

/// A QR code as one SVG path: dark modules on a `size` × `size` grid, quiet zone included.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Qr {
    pub size: u32,
    pub path: String,
}

/// Tailscale's addresses (CGNAT, 100.64.0.0/10).
fn is_tailscale(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (64..128).contains(&b)
}

/// Interfaces of virtual machines and containers on `os`, which nobody browses from.
fn is_virtual(os: Os, name: &str) -> bool {
    let prefixes: &[&str] = match os {
        Os::Linux => &["docker", "br-", "veth", "virbr", "lxcbr", "cni", "flannel", "vmnet"],
        Os::Windows => &["vEthernet", "VirtualBox", "VMware"],
        Os::Macos => &["bridge", "utun", "awdl", "llw", "vmnet"],
    };
    prefixes.iter().any(|p| name.starts_with(p))
}

fn kind_of(ip: IpAddr) -> UrlKind {
    match ip {
        IpAddr::V4(v4) if is_tailscale(v4) => UrlKind::Tailscale,
        ip if ip.is_loopback() => UrlKind::Loopback,
        _ => UrlKind::Lan,
    }
}

fn url_for(host: IpAddr, port: u16) -> String {
    match host {
        IpAddr::V4(v4) => format!("http://{v4}:{port}/"),
        IpAddr::V6(v6) => format!("http://[{v6}]:{port}/"),
    }
}

fn remote_url(url: String, kind: UrlKind) -> RemoteUrl {
    let qr_url = format!("{url}{PHONE_PAGE}");
    let qr = qr(&qr_url);
    RemoteUrl { url, kind, qr_url, qr }
}

/// Where the UI can be opened while listening on `bind`: Tailscale first, then the LAN, then this
/// computer. `ifaces` are the interfaces' names and addresses; on all of them (`0.0.0.0`) only
/// IPv4 is reachable, and virtual machines' and containers' interfaces are left out.
pub fn remote_urls(os: Os, bind: &str, port: u16, ifaces: &[(String, IpAddr)]) -> Vec<RemoteUrl> {
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut hosts: Vec<IpAddr> = match scope_of(bind) {
        Scope::Loopback => vec![local],
        Scope::Custom => {
            return match bind.parse::<IpAddr>() {
                Ok(ip) => vec![remote_url(url_for(ip, port), kind_of(ip))],
                Err(_) => vec![remote_url(format!("http://{bind}:{port}/"), UrlKind::Lan)],
            };
        }
        Scope::Network => ifaces
            .iter()
            .filter_map(|(name, ip)| match ip {
                IpAddr::V4(v4) if is_tailscale(*v4) => Some(*ip),
                IpAddr::V4(v4) if v4.is_link_local() || v4.is_unspecified() || is_virtual(os, name) => None,
                IpAddr::V4(_) => Some(*ip),
                IpAddr::V6(_) => None,
            })
            .chain([local])
            .collect(),
    };
    hosts.sort_by_key(|ip| (kind_of(*ip), *ip));
    hosts.dedup();
    hosts.into_iter().map(|ip| remote_url(url_for(ip, port), kind_of(ip))).collect()
}

/// Modules of empty border around a QR code, as the standard asks.
const QUIET: usize = 4;

/// `text` as a QR code (medium error correction), each row's dark runs as rectangles.
pub fn qr(text: &str) -> Qr {
    let Ok(code) = QrCode::with_error_correction_level(text, EcLevel::M) else {
        return Qr { size: 0, path: String::new() };
    };
    let w = code.width();
    let colors = code.to_colors();
    let mut path = String::new();
    for (y, row) in colors.chunks(w).enumerate() {
        let mut x = 0;
        while x < w {
            if row[x] != Color::Dark {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && row[x] == Color::Dark {
                x += 1;
            }
            let run = x - start;
            path.push_str(&format!("M{} {}h{run}v1h-{run}z", start + QUIET, y + QUIET));
        }
    }
    Qr { size: (w + 2 * QUIET) as u32, path }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(bind: &str) -> RemoteChoice {
        RemoteChoice { enabled: true, bind: bind.into(), port: 8080 }
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn scopes() {
        for b in ["127.0.0.1", "localhost", "::1", "127.0.0.2"] {
            assert_eq!(scope_of(b), Scope::Loopback, "{b}");
        }
        assert_eq!(scope_of("0.0.0.0"), Scope::Network);
        assert_eq!(scope_of("::"), Scope::Network);
        assert_eq!(scope_of("100.101.102.103"), Scope::Custom);
        assert_eq!(scope_of("myhost"), Scope::Custom);
    }

    #[test]
    fn any_address_needs_only_a_port() {
        for bind in ["127.0.0.1", "0.0.0.0", "100.64.0.1"] {
            validate_remote(&choice(bind)).unwrap();
        }
        assert!(validate_remote(&choice(" ")).is_err());
        assert!(validate_remote(&RemoteChoice { port: 0, ..choice("127.0.0.1") }).is_err());
    }

    #[test]
    fn bind_failures_per_os() {
        for os in [Os::Linux, Os::Windows, Os::Macos] {
            assert_eq!(bind_failure(os, ErrorKind::AddrInUse), RemoteFailure::PortInUse);
            assert_eq!(bind_failure(os, ErrorKind::AddrNotAvailable), RemoteFailure::NoSuchAddress);
            assert_eq!(bind_failure(os, ErrorKind::Other), RemoteFailure::Other);
        }
        assert_eq!(bind_failure(Os::Windows, ErrorKind::PermissionDenied), RemoteFailure::PortReserved);
        assert_eq!(bind_failure(Os::Linux, ErrorKind::PermissionDenied), RemoteFailure::PortPrivileged);
        assert_eq!(bind_failure(Os::Macos, ErrorKind::PermissionDenied), RemoteFailure::PortPrivileged);
    }

    fn urls(os: Os, bind: &str, ifaces: &[(&str, &str)]) -> Vec<(String, UrlKind)> {
        let ifaces: Vec<_> = ifaces.iter().map(|(n, a)| (n.to_string(), ip(a))).collect();
        remote_urls(os, bind, 8080, &ifaces).into_iter().map(|u| (u.url, u.kind)).collect()
    }

    #[test]
    fn network_urls_put_tailscale_first_and_drop_virtual_interfaces() {
        let linux = [
            ("lo", "127.0.0.1"),
            ("eth0", "192.168.1.20"),
            ("docker0", "172.17.0.1"),
            ("veth12ab", "172.18.0.1"),
            ("tailscale0", "100.101.102.103"),
            ("eth0", "fe80::1"),
            ("wlan0", "169.254.3.4"),
        ];
        assert_eq!(
            urls(Os::Linux, "0.0.0.0", &linux),
            [
                ("http://100.101.102.103:8080/".into(), UrlKind::Tailscale),
                ("http://192.168.1.20:8080/".into(), UrlKind::Lan),
                ("http://127.0.0.1:8080/".into(), UrlKind::Loopback),
            ]
        );
        let windows = [
            ("Ethernet", "10.0.0.5"),
            ("vEthernet (WSL)", "172.20.0.1"),
            ("Tailscale", "100.64.0.9"),
            ("Loopback Pseudo-Interface 1", "127.0.0.1"),
        ];
        let w = urls(Os::Windows, "0.0.0.0", &windows);
        assert_eq!(w.iter().map(|u| u.1).collect::<Vec<_>>(), [UrlKind::Tailscale, UrlKind::Lan, UrlKind::Loopback]);
        assert!(w[1].0.contains("10.0.0.5"));
        // macOS: Tailscale's own utun is kept by its address, other utuns are dropped
        let mac =
            [("en0", "192.168.0.7"), ("utun3", "100.88.1.2"), ("utun0", "10.8.0.2"), ("bridge100", "192.168.64.1")];
        assert_eq!(
            urls(Os::Macos, "0.0.0.0", &mac).into_iter().map(|u| u.0).collect::<Vec<_>>(),
            ["http://100.88.1.2:8080/", "http://192.168.0.7:8080/", "http://127.0.0.1:8080/"]
        );
    }

    #[test]
    fn loopback_and_custom_binds_offer_one_url() {
        let ifaces = [("eth0", "192.168.1.20")];
        assert_eq!(urls(Os::Linux, "127.0.0.1", &ifaces), [("http://127.0.0.1:8080/".into(), UrlKind::Loopback)]);
        assert_eq!(urls(Os::Linux, "localhost", &ifaces), [("http://127.0.0.1:8080/".into(), UrlKind::Loopback)]);
        assert_eq!(urls(Os::Linux, "100.100.1.1", &ifaces), [("http://100.100.1.1:8080/".into(), UrlKind::Tailscale)]);
        assert_eq!(urls(Os::Linux, "fd00::5", &ifaces), [("http://[fd00::5]:8080/".into(), UrlKind::Lan)]);
        assert_eq!(urls(Os::Linux, "pi.local", &ifaces), [("http://pi.local:8080/".into(), UrlKind::Lan)]);
    }

    #[test]
    fn qr_codes_open_the_phone_remote() {
        let ifaces = [("eth0".to_string(), ip("192.168.1.20"))];
        let u = &remote_urls(Os::Linux, "0.0.0.0", 8080, &ifaces)[0];
        assert_eq!(u.url, "http://192.168.1.20:8080/");
        assert_eq!(u.qr_url, "http://192.168.1.20:8080/remote.html");
        assert_eq!(u.qr, qr(&u.qr_url));
        let v6 = &remote_urls(Os::Linux, "fd00::5", 8080, &[])[0];
        assert_eq!(v6.qr_url, "http://[fd00::5]:8080/remote.html");
    }

    #[test]
    fn qr_is_a_square_of_runs_inside_the_quiet_zone() {
        let q = qr("http://192.168.1.20:8080/");
        // version 2 (25 modules) + the quiet zone
        assert_eq!(q.size, 25 + 2 * QUIET as u32);
        assert!(q.path.starts_with("M4 4h7v1h-7z"), "finder pattern's top row: {}", &q.path[..20]);
        for rect in q.path.split('z').filter(|r| !r.is_empty()) {
            let nums: Vec<u32> = rect
                .trim_start_matches('M')
                .split(['h', 'v', ' '])
                .filter_map(|n| n.trim_start_matches('-').parse().ok())
                .collect();
            let (x, y, run) = (nums[0], nums[1], nums[2]);
            assert!(x >= 4 && y >= 4 && x + run <= q.size - 4 && y < q.size - 4, "{rect}");
        }
    }
}
