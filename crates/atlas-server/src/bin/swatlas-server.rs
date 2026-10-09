//! Headless SW Atlas: rig + remote browser UI, no desktop window.
//! The systemd-friendly successor of the original Python prototype.
//!
//! Config: `$SWATLAS_CONFIG`, else `~/.config/swatlas/swatlas.toml`, plus env overrides
//! (`SWATLAS_RIG`, `RIGCTLD_HOST/PORT`, `WEB_BIND/PORT/AUTH`, `AUDIO_DEVICE/RATE`, ...).

use std::path::PathBuf;
use std::process::ExitCode;

use std::sync::Arc;

use atlas_core::update::Packaging;
use atlas_server::config::config_dir;
use atlas_server::update::{ManifestUpdater, CURRENT};
use atlas_server::{server, AppConfig, Atlas};

#[tokio::main]
async fn main() -> ExitCode {
    if std::env::args().nth(1).is_some_and(|a| a == "--version" || a == "-V") {
        println!("swatlas-server {CURRENT}");
        return ExitCode::SUCCESS;
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let path =
        std::env::var_os("SWATLAS_CONFIG").map(PathBuf::from).unwrap_or_else(|| config_dir().join("swatlas.toml"));
    let mut cfg = match AppConfig::load(&path).and_then(|c| c.validate_server().map(|_| c)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("swatlas-server: {e}");
            return ExitCode::FAILURE;
        }
    };
    cfg.quiet_dev_updates(cfg!(debug_assertions));
    let server_cfg = cfg.server.clone();
    // the server only says that an update exists: it is replaced by hand
    let updater = Arc::new(ManifestUpdater::new(Packaging::Server));
    let atlas = match Atlas::start(cfg, Some(path), Some(config_dir().join("qth.json")), updater) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("swatlas-server: {e}");
            return ExitCode::FAILURE;
        }
    };
    tokio::select! {
        r = server::serve(atlas.clone(), server_cfg) => {
            if let Err(e) = r {
                eprintln!("swatlas-server: {e}");
                return ExitCode::FAILURE;
            }
        }
        _ = shutdown() => tracing::info!("stopping"),
    }
    atlas.shutdown().await;
    ExitCode::SUCCESS
}

/// Ctrl-C, or the service manager stopping us: SIGTERM (systemd, launchd) or the console
/// closing / the system shutting down (Windows).
#[cfg(unix)]
async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

#[cfg(windows)]
async fn shutdown() {
    use tokio::signal::windows::{ctrl_close, ctrl_shutdown};
    let mut close = ctrl_close().expect("console close handler");
    let mut off = ctrl_shutdown().expect("shutdown handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = close.recv() => {}
        _ = off.recv() => {}
    }
}
