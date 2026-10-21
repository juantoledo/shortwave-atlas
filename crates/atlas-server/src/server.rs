//! HTTP + WebSocket server for remote browsers, started and stopped by `RemoteServer`.
//!
//! - `GET /api/ws`: `ServerMsg::State` on connect and on every change; `ClientMsg` calls
//!   answered with `ServerMsg::Reply`.
//! - `GET /api/audio`: endless raw PCM (s16le, mono, rate in `X-Audio-Rate`) while audio is enabled
//!   (the desktop app gets the same blocks over its `audio_open` channel).
//! - everything else: the built UI, from a directory or (desktop) the app's embedded assets.
//!
//! Remote browsers may not install updates or change remote access: `Call::InstallUpdate`
//! restarts the app on the host, and `Call::SetRemote` could cut them off, so only the desktop
//! window can send them.

use std::convert::Infallible;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use atlas_core::api::{ApiError, Call, ClientMsg, ErrorKind, ServerMsg};
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch};
use tower_http::services::{ServeDir, ServeFile};

use crate::app::Atlas;
use crate::audio::OpenError;

/// One file of the UI: its bytes and media type.
pub struct UiAsset {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// The desktop app's embedded UI, by path (`index.html`, `assets/x.js`), falling back to
/// `index.html` for the UI's own routes.
pub type Embedded = Arc<dyn Fn(&str) -> Option<UiAsset> + Send + Sync>;

/// Where the built UI comes from.
#[derive(Clone)]
pub enum UiSource {
    Dir(PathBuf),
    Embedded(Embedded),
}

/// `server.ui_dir`, else the app's embedded UI, else `ui/dist` under the working directory, else
/// `ui` next to the binary.
pub fn ui_source(ui_dir: Option<&Path>, embedded: Option<Embedded>) -> Option<UiSource> {
    let has_index = |d: &Path| d.join("index.html").is_file();
    if let Some(d) = ui_dir.filter(|d| has_index(d)) {
        return Some(UiSource::Dir(d.to_path_buf()));
    }
    if let Some(e) = embedded {
        return Some(UiSource::Embedded(e));
    }
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("ui")));
    [PathBuf::from("ui/dist")].into_iter().chain(exe_dir).find(|d| has_index(d)).map(UiSource::Dir)
}

/// What a running server shares with `RemoteServer`: its stop signal and how many browsers are
/// connected.
#[derive(Clone)]
pub struct Live {
    pub stop: watch::Receiver<bool>,
    pub clients: Arc<AtomicU32>,
}

impl Live {
    /// Resolves once the server stops (or whoever could stop it is gone).
    pub(crate) async fn stopped(mut self) {
        let _ = self.stop.wait_for(|s| *s).await;
    }
}

/// A connected browser, counted while it lasts.
struct Connected(Arc<AtomicU32>);

impl Connected {
    fn new(clients: &Arc<AtomicU32>) -> Self {
        clients.fetch_add(1, Ordering::Relaxed);
        Self(clients.clone())
    }
}

impl Drop for Connected {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
struct Ctx {
    atlas: Arc<Atlas>,
    live: Live,
}

/// No password: whoever reaches the port can use the app (the page warns beyond loopback).
pub fn router(atlas: Arc<Atlas>, ui: Option<UiSource>, live: Live) -> Router {
    let ctx = Ctx { atlas, live };
    let static_files = match ui {
        Some(UiSource::Dir(dir)) => {
            Router::new().fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(dir.join("index.html"))))
        }
        Some(UiSource::Embedded(get)) => Router::new().fallback(move |uri: Uri| async move { embedded(&get, &uri) }),
        None => Router::new().fallback(|| async {
            (StatusCode::NOT_FOUND, "Shortwave Atlas: the UI is not built. Run `npm --prefix ui run build`.")
        }),
    };
    Router::new().route("/api/ws", get(ws)).route("/api/audio", get(audio_stream)).merge(static_files).with_state(ctx)
}

/// A file of the embedded UI. Paths stay inside it: no `..`.
fn embedded(get: &Embedded, uri: &Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let inside = !path.contains('\\') && Path::new(path).components().all(|c| matches!(c, Component::Normal(_)));
    let Some(asset) = inside.then(|| get(path)).flatten() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // Vite names the other files by their contents; the page itself must not be kept
    let cache = if asset.mime.starts_with("text/html") { "no-cache" } else { "public, max-age=604800" };
    Response::builder()
        .header(header::CONTENT_TYPE, asset.mime)
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(asset.bytes))
        .expect("valid response")
}

async fn ws(State(ctx): State<Ctx>, up: WebSocketUpgrade) -> Response {
    up.on_upgrade(move |socket| session(socket, ctx.atlas, ctx.live))
}

async fn session(socket: WebSocket, atlas: Arc<Atlas>, live: Live) {
    let _counted = Connected::new(&live.clients);
    let stopped = live.stopped();
    tokio::pin!(stopped);
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerMsg>(64);
    let mut states = atlas.rig.subscribe();
    states.mark_changed(); // send the current state right away

    let writer = tokio::spawn(async move {
        loop {
            let msg = tokio::select! {
                m = rx.recv() => match m { Some(m) => m, None => break },
                r = states.changed() => {
                    if r.is_err() { break }
                    ServerMsg::State { state: states.borrow_and_update().clone() }
                }
            };
            let text = serde_json::to_string(&msg).expect("ServerMsg serialises");
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    loop {
        let msg = tokio::select! {
            m = stream.next() => match m { Some(Ok(m)) => m, _ => break },
            _ = &mut stopped => break,
        };
        let Message::Text(text) = msg else { continue };
        let Ok(ClientMsg { id, call }) = serde_json::from_str::<ClientMsg>(&text) else {
            tracing::debug!("ignoring bad message: {text}");
            continue;
        };
        // one task per call: a slow power-on must not hold up lookups
        let (atlas, tx) = (atlas.clone(), tx.clone());
        tokio::spawn(async move {
            let result = match call {
                Call::InstallUpdate => Err(ApiError {
                    kind: ErrorKind::Invalid,
                    message: "Install updates from the Shortwave Atlas window on the host.".into(),
                }),
                Call::SetRemote(_) => Err(ApiError {
                    kind: ErrorKind::Invalid,
                    message: "Change remote access from the Shortwave Atlas window on the host.".into(),
                }),
                call => atlas.call(call).await,
            };
            let reply = match result {
                Ok(v) => ServerMsg::Reply { id, ok: Some(v), err: None },
                Err(e) => ServerMsg::Reply { id, ok: None, err: Some(e) },
            };
            let _ = tx.send(reply).await;
        });
    }
    writer.abort();
}

async fn audio_stream(State(ctx): State<Ctx>) -> Response {
    let sub = match ctx.atlas.audio.subscribe() {
        Ok(s) => s,
        Err(e @ OpenError::Disabled) => return (StatusCode::NOT_FOUND, e.to_string()).into_response(),
        Err(e) => return (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };
    let rate = sub.rate;
    let body = futures_util::stream::unfold(sub, |mut sub| async move {
        // ends when the capture stops, the audio is reconfigured, or no audio arrives for a while
        sub.next_block().await.map(|block| (Ok::<_, Infallible>(block), sub))
    })
    .take_until(ctx.live.clone().stopped());
    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header("x-audio-rate", rate)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(body))
        .expect("valid response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use tokio_tungstenite::tungstenite;

    /// Serve on a free port; it stops when the returned sender says so, or is dropped.
    async fn start(ui: Option<UiSource>) -> (u16, watch::Sender<bool>) {
        let atlas = Atlas::start(AppConfig::default(), None, None, crate::app::tests::no_updates()).unwrap();
        let (tx, rx) = watch::channel(false);
        let live = Live { stop: rx, clients: Arc::default() };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = router(atlas, ui, live);
        tokio::spawn(async move { axum::serve(listener, app).await });
        (port, tx)
    }

    /// Status code and the whole response of `GET path`.
    async fn get(port: u16, path: &str) -> (u16, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        let mut buf = vec![];
        s.read_to_end(&mut buf).await.unwrap();
        let text = String::from_utf8_lossy(&buf).into_owned();
        (text[9..12].parse().unwrap(), text)
    }

    #[tokio::test]
    async fn audio_needs_no_password_but_must_be_on() {
        let (port, _stop) = start(None).await;
        // audio is disabled in the default config
        assert_eq!(get(port, "/api/audio").await.0, 404);
    }

    #[tokio::test]
    async fn embedded_ui_is_served_and_stays_inside() {
        let files: Embedded = Arc::new(|path| {
            let (bytes, mime) = match path {
                "index.html" => ("<html>atlas</html>", "text/html"),
                "assets/app.js" => ("let x", "text/javascript"),
                _ => return None,
            };
            Some(UiAsset { bytes: bytes.into(), mime: mime.into() })
        });
        let (port, _stop) = start(Some(UiSource::Embedded(files))).await;
        let (status, page) = get(port, "/").await;
        assert_eq!(status, 200);
        assert!(
            page.contains("text/html") && page.contains("no-cache") && page.ends_with("<html>atlas</html>"),
            "{page}"
        );
        let (status, js) = get(port, "/assets/app.js").await;
        assert!(status == 200 && js.contains("text/javascript") && js.ends_with("let x"), "{js}");
        assert_eq!(get(port, "/assets/missing.js").await.0, 404);
        assert_eq!(get(port, "/assets/../../secret").await.0, 404);
    }

    #[tokio::test]
    async fn websocket_pushes_state_and_answers_calls() {
        let (port, _stop) = start(None).await;
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/api/ws")).await.unwrap();
        let first: ServerMsg = match ws.next().await.unwrap().unwrap() {
            tungstenite::Message::Text(t) => serde_json::from_str(&t).unwrap(),
            m => panic!("unexpected {m:?}"),
        };
        assert!(matches!(first, ServerMsg::State { .. }));

        let call = r#"{"id":7,"call":{"cmd":"search","args":{"limit":1}}}"#;
        ws.send(tungstenite::Message::Text(call.into())).await.unwrap();
        loop {
            let tungstenite::Message::Text(t) = ws.next().await.unwrap().unwrap() else { continue };
            if let ServerMsg::Reply { id, ok, err } = serde_json::from_str(&t).unwrap() {
                assert_eq!((id, err), (7, None));
                let page = ok.unwrap();
                assert!(page["total"].as_u64().unwrap() > 1, "{page}");
                assert_eq!(page["items"].as_array().unwrap().len(), 1);
                break;
            }
        }

        // remote browsers see updates but can't install them on the host
        let call = r#"{"id":8,"call":{"cmd":"install_update"}}"#;
        ws.send(tungstenite::Message::Text(call.into())).await.unwrap();
        loop {
            let tungstenite::Message::Text(t) = ws.next().await.unwrap().unwrap() else { continue };
            if let ServerMsg::Reply { id, err, .. } = serde_json::from_str(&t).unwrap() {
                assert_eq!((id, err.map(|e| e.kind)), (8, Some(ErrorKind::Invalid)));
                break;
            }
        }

        // nor change remote access under their own feet
        let call = r#"{"id":9,"call":{"cmd":"set_remote","args":{"enabled":false,"bind":"127.0.0.1","port":8080}}}"#;
        ws.send(tungstenite::Message::Text(call.into())).await.unwrap();
        loop {
            let tungstenite::Message::Text(t) = ws.next().await.unwrap().unwrap() else { continue };
            if let ServerMsg::Reply { id, err, .. } = serde_json::from_str(&t).unwrap() {
                assert_eq!((id, err.map(|e| e.kind)), (9, Some(ErrorKind::Invalid)));
                break;
            }
        }
    }
}
