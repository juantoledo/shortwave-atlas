//! Shortwave Atlas app core and remote server: config, the `Atlas` dispatcher shared by the
//! Tauri shell and the WebSocket, rig audio, and the HTTP server.

pub mod app;
pub mod audio;
pub mod config;
pub mod server;
pub mod stations;
pub mod update;

pub use app::Atlas;
pub use config::AppConfig;
