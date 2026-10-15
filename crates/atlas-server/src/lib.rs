//! Shortwave Atlas app core and remote server: config, the `Atlas` dispatcher shared by the
//! Tauri shell and the WebSocket, rig audio, and the HTTP server.

pub mod app;
pub mod audio;
pub mod config;
pub mod favorites;
pub mod files;
pub mod http;
pub mod listening;
pub mod net;
pub mod scan;
pub mod server;
pub mod sources;
pub mod update;
pub mod user_stations;

pub use app::Atlas;
pub use config::AppConfig;
