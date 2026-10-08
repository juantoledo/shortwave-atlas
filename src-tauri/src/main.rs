//! SW Atlas desktop shell (Tauri v2).
//!
//! The UI calls the core through one command, `call`, carrying the same `Call`s the
//! remote browser sends over the WebSocket; rig state changes arrive as `rig://state`.
//! Rig audio comes over a binary channel (`audio_open`/`audio_close`), the counterpart of
//! the browser's `GET /api/audio`, fed by the same `AudioHub`.
//! With `server.enabled`, the same process also serves the remote browser UI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use atlas_core::api::{ApiError, Call, ErrorKind};
use atlas_server::config::config_dir;
use atlas_server::{server, AppConfig, Atlas};
use tauri::async_runtime::JoinHandle;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{Emitter, Manager, RunEvent, State};

#[tauri::command]
async fn call(atlas: State<'_, Arc<Atlas>>, call: Call) -> Result<serde_json::Value, ApiError> {
    tracing::debug!(?call, "ui call");
    atlas.call(call).await
}

/// Audio streams open in this window, by id.
#[derive(Default)]
struct AudioStreams {
    next: AtomicU32,
    tasks: Mutex<HashMap<u32, JoinHandle<()>>>,
}

#[derive(serde::Serialize)]
struct AudioStream {
    id: u32,
    /// Sample rate of the PCM on this stream.
    rate: u32,
}

/// Start sending rig audio (raw s16le mono PCM blocks) on `on_block`. One empty block
/// marks the end of the stream (ffmpeg stopped, audio reconfigured, or stalled); the UI
/// then closes it and opens a new one, like a browser reconnecting to `/api/audio`.
#[tauri::command]
async fn audio_open(
    atlas: State<'_, Arc<Atlas>>,
    streams: State<'_, AudioStreams>,
    on_block: Channel<InvokeResponseBody>,
) -> Result<AudioStream, ApiError> {
    let mut sub =
        atlas.audio.subscribe().map_err(|e| ApiError { kind: ErrorKind::Conflict, message: e.to_string() })?;
    let (id, rate) = (streams.next.fetch_add(1, Ordering::Relaxed), sub.rate);
    let task = tauri::async_runtime::spawn(async move {
        while let Some(block) = sub.next_block().await {
            if on_block.send(InvokeResponseBody::Raw(block.to_vec())).is_err() {
                return;
            }
        }
        let _ = on_block.send(InvokeResponseBody::Raw(vec![]));
    });
    streams.tasks.lock().expect("audio streams lock").insert(id, task);
    Ok(AudioStream { id, rate })
}

/// Stop an audio stream (dropping its subscription stops ffmpeg if nobody else listens).
#[tauri::command]
fn audio_close(streams: State<'_, AudioStreams>, id: u32) {
    if let Some(task) = streams.tasks.lock().expect("audio streams lock").remove(&id) {
        task.abort();
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        .setup(|app| {
            // Same config as swatlas-server, so both can share one machine's settings.
            let dir = config_dir();
            let path = std::env::var_os("SWATLAS_CONFIG").map(Into::into).unwrap_or_else(|| dir.join("swatlas.toml"));
            let cfg = AppConfig::load(&path)?;
            if cfg.server.enabled {
                cfg.validate_server()?;
            }
            let server_cfg = cfg.server.clone();
            // Atlas spawns its tasks on Tauri's tokio runtime.
            let atlas =
                tauri::async_runtime::block_on(async { Atlas::start(cfg, Some(path), Some(dir.join("qth.json"))) })?;

            let mut states = atlas.rig.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while states.changed().await.is_ok() {
                    let s = states.borrow_and_update().clone();
                    let _ = handle.emit("rig://state", s);
                }
            });

            if server_cfg.enabled {
                let atlas = atlas.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = server::serve(atlas, server_cfg).await {
                        tracing::error!("remote server: {e}");
                    }
                });
            }
            app.manage(atlas);
            app.manage(AudioStreams::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![call, audio_open, audio_close])
        .build(tauri::generate_context!())
        .expect("error while running SW Atlas")
        .run(|app, event| {
            // Tauri exits the process without dropping state, so stop rigctld and ffmpeg here
            if let RunEvent::Exit = event {
                if let Some(atlas) = app.try_state::<Arc<Atlas>>() {
                    tauri::async_runtime::block_on(atlas.shutdown());
                }
            }
        });
}
