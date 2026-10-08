//! "Update available": the release manifest, version comparison, how this build can be
//! updated (per OS and packaging), and the update state the UI shows. No IO: the server
//! fetches the manifest and the desktop shell installs (Tauri updater).

use std::collections::BTreeMap;

use semver::Version;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::platform::Os;

/// Every release, for a manual download when no release page is known.
pub const RELEASES_PAGE: &str = "https://github.com/juantoledo/shortwave-atlas/releases";
/// First check this long after start (let the rig and the globe come up first).
pub const FIRST_CHECK_SECS: u64 = 30;
/// Then check this often.
pub const CHECK_EVERY_SECS: u64 = 6 * 3600;

/// Which releases to offer. The manifests are written by `.github/workflows/promote.yml`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Channel {
    /// Published releases only.
    #[default]
    Stable,
    /// Release candidates too (`vX.Y.Z-rc.N`).
    Beta,
}

impl Channel {
    pub fn manifest_url(self) -> &'static str {
        match self {
            Channel::Stable => "https://juantoledo.github.io/shortwave-atlas/updates/stable.json",
            Channel::Beta => "https://juantoledo.github.io/shortwave-atlas/updates/beta.json",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }
}

/// A release offered by a manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Release {
    /// Without the tag's `v`: `0.2.0`, `0.2.0-rc.1`.
    pub version: String,
    /// Release notes (Markdown), empty if there are none.
    pub notes: String,
    /// RFC 3339, as written by the release workflow.
    pub pub_date: Option<String>,
    /// The release page, for a manual download.
    pub url: String,
}

/// Tauri's `latest.json` (as written by `tauri-action`).
#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    pub_date: Option<String>,
    #[serde(default)]
    platforms: BTreeMap<String, ManifestPlatform>,
}

#[derive(Deserialize)]
struct ManifestPlatform {
    url: String,
}

/// Read a release manifest. The version must be semver (an optional leading `v` is dropped).
pub fn parse_manifest(json: &str) -> Result<Release, String> {
    let m: Manifest = serde_json::from_str(json).map_err(|e| format!("bad update manifest: {e}"))?;
    let version = m.version.trim().trim_start_matches('v').to_string();
    Version::parse(&version).map_err(|e| format!("bad version in update manifest {:?}: {e}", m.version))?;
    let url = m.platforms.values().find_map(|p| release_page(&p.url)).unwrap_or_else(|| RELEASES_PAGE.into());
    Ok(Release { version, notes: m.notes.unwrap_or_default(), pub_date: m.pub_date, url })
}

/// `https://github.com/o/r/releases/download/v1.2.3/file` -> `https://github.com/o/r/releases/tag/v1.2.3`.
pub fn release_page(asset_url: &str) -> Option<String> {
    let (repo, rest) = asset_url.split_once("/releases/download/")?;
    let tag = rest.split('/').next().filter(|t| !t.is_empty())?;
    repo.starts_with("https://github.com/").then(|| format!("{repo}/releases/tag/{tag}"))
}

/// `candidate` is a newer version than `current` (semver: `0.2.0-rc.2` > `0.2.0-rc.1`,
/// `0.2.0` > `0.2.0-rc.9`). Anything unparseable is not newer.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    let v = |s: &str| Version::parse(s.trim().trim_start_matches('v')).ok();
    matches!((v(current), v(candidate)), (Some(c), Some(n)) if n > c)
}

/// How this copy of SW Atlas was installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Packaging {
    /// The desktop app. `appimage`: running from an AppImage (the `APPIMAGE` variable is set).
    Desktop { appimage: bool },
    /// `swatlas-server`, from an archive (often run by a service manager).
    Server,
}

/// Why an update has to be installed by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ManualReason {
    /// `swatlas-server`: replace the files and restart the service.
    Server,
    /// Linux `.deb`: install the new package.
    LinuxPackage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum InstallMode {
    /// The app downloads, verifies and installs the update, then restarts.
    Auto,
    /// The app only says that an update exists and links to it.
    Manual { reason: ManualReason },
}

/// Only the desktop app installs updates itself, and on Linux only as an AppImage (a
/// `.deb` belongs to the package manager).
pub fn install_mode(os: Os, packaging: Packaging) -> InstallMode {
    match packaging {
        Packaging::Server => InstallMode::Manual { reason: ManualReason::Server },
        Packaging::Desktop { appimage: false } if os == Os::Linux => {
            InstallMode::Manual { reason: ManualReason::LinuxPackage }
        }
        Packaging::Desktop { .. } => InstallMode::Auto,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum UpdateState {
    /// Automatic checks are off (a manual check still works).
    Disabled,
    /// No check yet.
    Idle,
    Checking,
    UpToDate,
    Available {
        release: Release,
    },
    Downloading {
        release: Release,
        #[ts(type = "number")]
        done: u64,
        #[ts(type = "number | null")]
        total: Option<u64>,
    },
    /// Downloaded and verified; the app is stopping its helpers and installing.
    Installing {
        release: Release,
    },
    /// The last check or install failed; `release` is set when an install can be retried.
    Failed {
        message: String,
        release: Option<Release>,
    },
}

/// What happened, for `UpdateState::next`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateEvent {
    /// Automatic checks turned on or off.
    Enabled(bool),
    CheckStarted,
    /// The check finished: a newer release, or none.
    Found(Option<Release>),
    CheckFailed(String),
    DownloadStarted,
    Progress {
        done: u64,
        total: Option<u64>,
    },
    InstallStarted,
    InstallFailed(String),
}

impl UpdateState {
    /// A download or install is running: checks and settings must not change the state.
    pub fn busy(&self) -> bool {
        matches!(self, UpdateState::Downloading { .. } | UpdateState::Installing { .. })
    }

    /// The release an install would fetch now (`Available`, or a failed install to retry).
    pub fn installable(&self) -> Option<&Release> {
        match self {
            UpdateState::Available { release } | UpdateState::Failed { release: Some(release), .. } => Some(release),
            _ => None,
        }
    }

    fn release(&self) -> Option<&Release> {
        match self {
            UpdateState::Available { release }
            | UpdateState::Downloading { release, .. }
            | UpdateState::Installing { release }
            | UpdateState::Failed { release: Some(release), .. } => Some(release),
            _ => None,
        }
    }

    pub fn next(self, ev: UpdateEvent) -> UpdateState {
        use UpdateEvent as E;
        use UpdateState as S;
        match (self, ev) {
            // nothing interrupts a download or install except its own progress and outcome
            (S::Downloading { release, .. }, E::Progress { done, total }) => S::Downloading { release, done, total },
            (S::Downloading { release, .. }, E::InstallStarted) => S::Installing { release },
            (s, E::InstallFailed(message)) if s.busy() => S::Failed { message, release: s.release().cloned() },
            (s, _) if s.busy() => s,

            (S::Disabled, E::Enabled(true)) => S::Idle,
            (_, E::Enabled(false)) => S::Disabled,
            (s, E::Enabled(true)) => s,
            // a background re-check keeps an offered release on screen
            (s @ S::Available { .. }, E::CheckStarted) => s,
            (_, E::CheckStarted) => S::Checking,
            (_, E::Found(Some(release))) => S::Available { release },
            (_, E::Found(None)) => S::UpToDate,
            // a network hiccup doesn't hide an offered release
            (s @ S::Available { .. }, E::CheckFailed(_)) => s,
            (_, E::CheckFailed(message)) => S::Failed { message, release: None },
            (s, E::DownloadStarted) => match s.installable().cloned() {
                Some(release) => S::Downloading { release, done: 0, total: None },
                None => s,
            },
            (s, E::Progress { .. } | E::InstallStarted | E::InstallFailed(_)) => s,
        }
    }
}

/// Result of `Call::UpdateStatus`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateStatus {
    /// This build's version.
    pub current: String,
    /// Check automatically (at start, then every few hours).
    pub check: bool,
    pub channel: Channel,
    pub install: InstallMode,
    pub state: UpdateState,
    /// Unix seconds of the last finished check.
    #[ts(type = "number | null")]
    pub checked_at: Option<u64>,
    /// Settings forced by environment variables (`check`): a change lasts until restart.
    pub locked: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape of the `latest.json` that `tauri-action` uploads.
    const MANIFEST: &str = r#"{
      "version": "0.2.0",
      "notes": "Features:\n\n* update available banner",
      "pub_date": "2026-10-20T12:00:00.000Z",
      "platforms": {
        "darwin-aarch64": { "signature": "dW50cnVzdGVk", "url": "https://github.com/juantoledo/shortwave-atlas/releases/download/v0.2.0/SW.Atlas_universal.app.tar.gz" },
        "linux-x86_64": { "signature": "dW50cnVzdGVk", "url": "https://github.com/juantoledo/shortwave-atlas/releases/download/v0.2.0/SW.Atlas_0.2.0_amd64.AppImage" },
        "windows-x86_64": { "signature": "dW50cnVzdGVk", "url": "https://github.com/juantoledo/shortwave-atlas/releases/download/v0.2.0/SW.Atlas_0.2.0_x64-setup.exe" }
      }
    }"#;

    fn rel(v: &str) -> Release {
        Release { version: v.into(), notes: String::new(), pub_date: None, url: RELEASES_PAGE.into() }
    }

    #[test]
    fn reads_tauri_manifest() {
        let r = parse_manifest(MANIFEST).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert!(r.notes.contains("update available"));
        assert_eq!(r.pub_date.as_deref(), Some("2026-10-20T12:00:00.000Z"));
        assert_eq!(r.url, "https://github.com/juantoledo/shortwave-atlas/releases/tag/v0.2.0");
    }

    #[test]
    fn manifest_edge_cases() {
        let r = parse_manifest(r#"{"version":"v0.3.0-rc.1"}"#).unwrap();
        assert_eq!((r.version.as_str(), r.notes.as_str(), r.url.as_str()), ("0.3.0-rc.1", "", RELEASES_PAGE));
        assert!(parse_manifest(r#"{"version":"latest"}"#).is_err());
        assert!(parse_manifest("<html>404</html>").is_err());
    }

    #[test]
    fn release_pages() {
        assert_eq!(
            release_page("https://github.com/o/r/releases/download/v1.0.0/a.exe").unwrap(),
            "https://github.com/o/r/releases/tag/v1.0.0"
        );
        assert_eq!(release_page("https://example.com/o/r/releases/download/v1/a"), None);
        assert_eq!(release_page("https://github.com/o/r/releases/download//a"), None);
    }

    #[test]
    fn version_order() {
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(is_newer("0.2.0-rc.1", "0.2.0-rc.2"));
        assert!(is_newer("0.2.0-rc.9", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0-rc.3"));
        assert!(!is_newer("0.2.0", "0.1.9"));
        assert!(!is_newer("0.2.0", "garbage"));
    }

    #[test]
    fn channel_urls() {
        assert!(Channel::Stable.manifest_url().ends_with("/updates/stable.json"));
        assert!(Channel::Beta.manifest_url().ends_with("/updates/beta.json"));
        assert_eq!(serde_json::to_string(&Channel::Beta).unwrap(), r#""beta""#);
    }

    #[test]
    fn install_modes_per_os() {
        let manual = |reason| InstallMode::Manual { reason };
        for os in [Os::Linux, Os::Windows, Os::Macos] {
            assert_eq!(install_mode(os, Packaging::Server), manual(ManualReason::Server), "{os:?}");
            assert_eq!(install_mode(os, Packaging::Desktop { appimage: true }), InstallMode::Auto, "{os:?}");
        }
        assert_eq!(install_mode(Os::Linux, Packaging::Desktop { appimage: false }), manual(ManualReason::LinuxPackage));
        assert_eq!(install_mode(Os::Windows, Packaging::Desktop { appimage: false }), InstallMode::Auto);
        assert_eq!(install_mode(Os::Macos, Packaging::Desktop { appimage: false }), InstallMode::Auto);
        assert_eq!(
            serde_json::to_value(manual(ManualReason::LinuxPackage)).unwrap(),
            serde_json::json!({"kind": "manual", "reason": "linux_package"})
        );
    }

    #[test]
    fn check_states() {
        use UpdateEvent as E;
        let s = UpdateState::Idle.next(E::CheckStarted);
        assert_eq!(s, UpdateState::Checking);
        assert_eq!(s.clone().next(E::Found(None)), UpdateState::UpToDate);
        let avail = s.next(E::Found(Some(rel("0.2.0"))));
        assert_eq!(avail, UpdateState::Available { release: rel("0.2.0") });
        // background re-checks and their failures keep the offer
        assert_eq!(avail.clone().next(E::CheckStarted), avail);
        assert_eq!(avail.clone().next(E::CheckFailed("offline".into())), avail);
        assert_eq!(
            UpdateState::Checking.next(E::CheckFailed("offline".into())),
            UpdateState::Failed { message: "offline".into(), release: None }
        );
        assert_eq!(avail.clone().next(E::Enabled(false)), UpdateState::Disabled);
        assert_eq!(UpdateState::Disabled.next(E::Enabled(true)), UpdateState::Idle);
        assert_eq!(UpdateState::UpToDate.next(E::Enabled(true)), UpdateState::UpToDate);
        // a manual check works while automatic checks are off
        assert_eq!(UpdateState::Disabled.next(E::CheckStarted), UpdateState::Checking);
    }

    #[test]
    fn install_states() {
        use UpdateEvent as E;
        assert_eq!(UpdateState::UpToDate.next(E::DownloadStarted), UpdateState::UpToDate, "nothing to install");
        let avail = UpdateState::Available { release: rel("0.2.0") };
        let dl = avail.next(E::DownloadStarted);
        assert_eq!(dl, UpdateState::Downloading { release: rel("0.2.0"), done: 0, total: None });
        let dl = dl.next(E::Progress { done: 10, total: Some(100) });
        assert_eq!(dl, UpdateState::Downloading { release: rel("0.2.0"), done: 10, total: Some(100) });
        // checks and settings don't interrupt
        for ev in [E::CheckStarted, E::Found(None), E::CheckFailed("x".into()), E::Enabled(false), E::DownloadStarted] {
            assert_eq!(dl.clone().next(ev.clone()), dl, "{ev:?}");
        }
        let inst = dl.next(E::InstallStarted);
        assert_eq!(inst, UpdateState::Installing { release: rel("0.2.0") });
        assert!(inst.busy());
        let failed = inst.next(E::InstallFailed("bad signature".into()));
        assert_eq!(failed, UpdateState::Failed { message: "bad signature".into(), release: Some(rel("0.2.0")) });
        // retry
        assert_eq!(failed.installable(), Some(&rel("0.2.0")));
        assert!(matches!(failed.next(E::DownloadStarted), UpdateState::Downloading { .. }));
    }
}
