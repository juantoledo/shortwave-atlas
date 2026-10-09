//! HTTP + WebSocket server for remote browsers.
//!
//! - `GET /api/ws`: `ServerMsg::State` on connect and on every change; `ClientMsg` calls
//!   answered with `ServerMsg::Reply`.
//! - `GET /api/audio`: endless raw PCM (s16le, mono, rate in `X-Audio-Rate`) while audio is enabled
//!   (the desktop app gets the same blocks over its `audio_open` channel).
//! - everything else: the built UI.
//!
//! Remote browsers may not install updates: `Call::InstallUpdate` restarts the app on the
//! host, so only the desktop window can send it.

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use atlas_core::api::{ApiError, Call, ClientMsg, ErrorKind, ServerMsg};
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::prelude::{Engine, BASE64_STANDARD};
use futures_util::{SinkExt, StreamExt};
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use tower_http::services::{ServeDir, ServeFile};

use crate::app::Atlas;
use crate::audio::OpenError;
use crate::config::ServerConfig;

#[derive(Clone)]
struct Ctx {
    atlas: Arc<Atlas>,
    auth: Option<Arc<str>>,
}

pub fn router(atlas: Arc<Atlas>, cfg: &ServerConfig) -> Router {
    let ctx = Ctx { atlas, auth: cfg.auth.as_deref().map(Arc::from) };
    let ui = ui_dir(cfg);
    let static_files = match &ui {
        Some(dir) => {
            Router::new().fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))))
        }
        None => Router::new().fallback(|| async {
            (StatusCode::NOT_FOUND, "Shortwave Atlas: the UI is not built. Run `npm --prefix ui run build`.")
        }),
    };
    Router::new()
        .route("/api/ws", get(ws))
        .route("/api/audio", get(audio_stream))
        .merge(static_files)
        .layer(middleware::from_fn_with_state(ctx.clone(), basic_auth))
        .with_state(ctx)
}

/// `server.ui_dir`, else `ui/dist` under the working directory, else `ui` next to the binary.
fn ui_dir(cfg: &ServerConfig) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("ui")));
    cfg.ui_dir
        .clone()
        .into_iter()
        .chain([PathBuf::from("ui/dist")])
        .chain(exe_dir)
        .find(|d| d.join("index.html").is_file())
}

async fn basic_auth(State(ctx): State<Ctx>, req: Request, next: Next) -> Response {
    let Some(expected) = &ctx.auth else { return next.run(req).await };
    let given = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Basic "))
        .and_then(|b| BASE64_STANDARD.decode(b).ok());
    match given {
        Some(g) if bool::from(g.ct_eq(expected.as_bytes())) => next.run(req).await,
        _ => {
            let mut r = StatusCode::UNAUTHORIZED.into_response();
            r.headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic realm=\"Shortwave Atlas\""));
            r
        }
    }
}

async fn ws(State(ctx): State<Ctx>, up: WebSocketUpgrade) -> Response {
    up.on_upgrade(move |socket| session(socket, ctx.atlas))
}

async fn session(socket: WebSocket, atlas: Arc<Atlas>) {
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

    while let Some(Ok(msg)) = stream.next().await {
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
        // ends when ffmpeg stops, the audio is reconfigured, or no audio arrives for a while
        sub.next_block().await.map(|block| (Ok::<_, Infallible>(block), sub))
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header("x-audio-rate", rate)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(body))
        .expect("valid response")
}

/// Listen and serve until the task is dropped.
pub async fn serve(atlas: Arc<Atlas>, cfg: ServerConfig) -> std::io::Result<()> {
    use axum::serve::ListenerExt;
    let listener = tokio::net::TcpListener::bind((cfg.bind.as_str(), cfg.port)).await?.tap_io(|tcp| {
        // audio blocks are small and latency matters more than packet count
        let _ = tcp.set_nodelay(true);
    });
    tracing::info!("Shortwave Atlas web at http://{}:{}", cfg.bind, cfg.port);
    axum::serve(listener, router(atlas, &cfg)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use tokio_tungstenite::tungstenite;

    async fn start(auth: Option<&str>) -> (u16, Arc<Atlas>) {
        let atlas = Atlas::start(AppConfig::default(), None, None, crate::app::tests::no_updates()).unwrap();
        let cfg = ServerConfig { auth: auth.map(String::from), ..ServerConfig::default() };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = router(atlas.clone(), &cfg);
        tokio::spawn(async move { axum::serve(listener, app).await });
        (port, atlas)
    }

    async fn get_status(port: u16, auth: Option<&str>) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let auth = auth.map(|a| format!("Authorization: Basic {}\r\n", BASE64_STANDARD.encode(a))).unwrap_or_default();
        s.write_all(format!("GET /api/audio HTTP/1.1\r\nHost: x\r\n{auth}Connection: close\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut buf = vec![0; 64];
        let n = s.read(&mut buf).await.unwrap();
        String::from_utf8_lossy(&buf[..n])[9..12].parse().unwrap()
    }

    #[tokio::test]
    async fn auth_is_enforced() {
        let (port, _) = start(Some("me:secret")).await;
        assert_eq!(get_status(port, None).await, 401);
        assert_eq!(get_status(port, Some("me:wrong")).await, 401);
        // authorised, but audio is disabled in the default config
        assert_eq!(get_status(port, Some("me:secret")).await, 404);
    }

    #[tokio::test]
    async fn websocket_pushes_state_and_answers_calls() {
        let (port, _) = start(None).await;
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
    }
}
