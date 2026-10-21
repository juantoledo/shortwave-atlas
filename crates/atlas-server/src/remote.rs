//! Remote access: start, restart and stop the browser UI's server while the app runs
//! (Settings → Remote access), and say where it can be opened. swatlas-server starts it once
//! and always serves.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use atlas_core::api::RemoteStatus;
use atlas_core::platform::Os;
use atlas_core::remote::{bind_failure, remote_urls, scope_of, validate_remote, RemoteChoice, RemoteFailure, Scope};
use axum::serve::ListenerExt;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;

use crate::app::Atlas;
use crate::server::{self, Embedded, Live};

/// How long a stop waits for connections to close before dropping them.
const STOP_GRACE: Duration = Duration::from_secs(2);

enum State {
    Off { last: RemoteChoice, failure: Option<(RemoteFailure, String)> },
    Running { choice: RemoteChoice, stop: watch::Sender<bool>, task: JoinHandle<()> },
}

pub struct RemoteServer {
    headless: AtomicBool,
    /// `server.ui_dir` from the config.
    ui_dir: Option<PathBuf>,
    embedded: RwLock<Option<Embedded>>,
    clients: Arc<AtomicU32>,
    state: Mutex<State>,
}

impl RemoteServer {
    /// Stopped, remembering `last` (the config's `[server]`) for the page.
    pub fn new(last: RemoteChoice, ui_dir: Option<PathBuf>) -> Self {
        Self {
            headless: AtomicBool::new(false),
            ui_dir,
            embedded: RwLock::new(None),
            clients: Arc::new(AtomicU32::new(0)),
            state: Mutex::new(State::Off { last, failure: None }),
        }
    }

    /// The desktop app's embedded UI, served unless `server.ui_dir` is set.
    pub fn set_embedded(&self, get: Embedded) {
        *self.embedded.write().expect("embedded lock") = Some(get);
    }

    /// swatlas-server: always serving; the page only shows where.
    pub fn headless(&self) -> bool {
        self.headless.load(Ordering::Relaxed)
    }

    /// Start serving for good (swatlas-server), whatever `choice.enabled` says.
    pub async fn serve_headless(&self, atlas: &Arc<Atlas>, choice: RemoteChoice) -> Result<(), String> {
        self.headless.store(true, Ordering::Relaxed);
        self.apply(atlas, RemoteChoice { enabled: true, ..choice }).await
    }

    /// Stop what runs, then start `choice` if it is enabled. A refused choice or a failed bind
    /// leaves it stopped, with the reason in `status`.
    pub async fn apply(&self, atlas: &Arc<Atlas>, choice: RemoteChoice) -> Result<(), String> {
        let mut state = self.state.lock().await;
        stop(&mut state, choice.clone()).await;
        if !choice.enabled {
            return Ok(());
        }
        let fail = |state: &mut State, failure, error: String| {
            *state = State::Off { last: choice.clone(), failure: Some((failure, error.clone())) };
            Err(error)
        };
        if let Err(e) = validate_remote(&choice) {
            return fail(&mut state, RemoteFailure::Invalid, e);
        }
        let listener = match tokio::net::TcpListener::bind((choice.bind.as_str(), choice.port)).await {
            Ok(l) => l,
            Err(e) => {
                let failure = bind_failure(Os::CURRENT, e.kind());
                return fail(&mut state, failure, format!("{}:{}: {e}", choice.bind, choice.port));
            }
        };
        let listener = listener.tap_io(|tcp| {
            // audio blocks are small and latency matters more than packet count
            let _ = tcp.set_nodelay(true);
        });
        let embedded = self.embedded.read().expect("embedded lock").clone();
        let ui = server::ui_source(self.ui_dir.as_deref(), embedded);
        let (tx, rx) = watch::channel(false);
        let live = Live { stop: rx, clients: self.clients.clone() };
        let app = server::router(atlas.clone(), ui, live.clone());
        tracing::info!("Shortwave Atlas web at http://{}:{}", choice.bind, choice.port);
        if scope_of(&choice.bind) != Scope::Loopback {
            tracing::warn!("remote access has no password: anyone who reaches {} can use the app", choice.bind);
        }
        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).with_graceful_shutdown(live.stopped()).await {
                tracing::error!("remote access: {e}");
            }
        });
        *state = State::Running { choice, stop: tx, task };
        Ok(())
    }

    /// Stop serving (on the way out), keeping the choice for the page.
    pub async fn stop(&self) {
        let mut state = self.state.lock().await;
        let last = match &*state {
            State::Off { .. } => return,
            State::Running { choice, .. } => choice.clone(),
        };
        stop(&mut state, last).await;
    }

    /// What runs and where to open it. `locked` and `config_path` are the caller's to fill.
    pub async fn status(&self) -> RemoteStatus {
        let state = self.state.lock().await;
        let (running, choice, failure) = match &*state {
            State::Off { last, failure } => (false, last.clone(), failure.clone()),
            State::Running { choice, .. } => (true, choice.clone(), None),
        };
        drop(state);
        let urls = if running { remote_urls(Os::CURRENT, &choice.bind, choice.port, &interfaces()) } else { vec![] };
        RemoteStatus {
            headless: self.headless(),
            running,
            scope: scope_of(&choice.bind),
            urls,
            clients: if running { self.clients.load(Ordering::Relaxed) } else { 0 },
            failure: failure.as_ref().map(|f| f.0),
            error: failure.map(|f| f.1),
            choice,
            locked: vec![],
            config_path: String::new(),
        }
    }
}

/// End the running server, if any: browsers are disconnected and the port is free once it
/// returns. `last` is what the page shows next.
async fn stop(state: &mut State, last: RemoteChoice) {
    let old = std::mem::replace(state, State::Off { last, failure: None });
    if let State::Running { stop, mut task, .. } = old {
        let _ = stop.send(true);
        if tokio::time::timeout(STOP_GRACE, &mut task).await.is_err() {
            task.abort();
            let _ = task.await;
        }
        tracing::info!("remote access stopped");
    }
}

/// This computer's interfaces and their addresses (none if they can't be listed).
fn interfaces() -> Vec<(String, IpAddr)> {
    match if_addrs::get_if_addrs() {
        Ok(list) => list.into_iter().map(|i| (i.name.clone(), i.ip())).collect(),
        Err(e) => {
            tracing::warn!("could not list the network interfaces: {e}");
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::no_updates;
    use crate::config::AppConfig;
    use atlas_core::api::{Call, ErrorKind};
    use futures_util::StreamExt;

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    fn local(enabled: bool, port: u16) -> RemoteChoice {
        RemoteChoice { enabled, bind: "127.0.0.1".into(), port }
    }

    async fn set(atlas: &Atlas, c: RemoteChoice) -> RemoteStatus {
        serde_json::from_value(atlas.call(Call::SetRemote(c)).await.unwrap()).unwrap()
    }

    async fn clients(atlas: &Atlas, want: u32) {
        for _ in 0..100 {
            if atlas.remote.status().await.clients == want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("never {want} clients");
    }

    #[tokio::test]
    async fn starts_counts_browsers_stops_and_frees_the_port() {
        let atlas = Atlas::start(AppConfig::default(), None, None, no_updates()).unwrap();
        let port = free_port();
        let s = set(&atlas, local(true, port)).await;
        assert!(s.running && !s.headless && s.failure.is_none(), "{s:?}");
        assert_eq!(s.urls.iter().map(|u| u.url.as_str()).collect::<Vec<_>>(), [format!("http://127.0.0.1:{port}/")]);

        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/api/ws")).await.unwrap();
        ws.next().await.unwrap().unwrap(); // the first state
        clients(&atlas, 1).await;

        let s = set(&atlas, local(false, port)).await;
        assert!(!s.running && s.urls.is_empty() && s.clients == 0, "{s:?}");
        // the browser is cut off, and the port is free again
        let closed = tokio::time::timeout(Duration::from_secs(5), async { while let Some(Ok(_)) = ws.next().await {} });
        closed.await.expect("the WebSocket stays open after a stop");
        clients(&atlas, 0).await;
        drop(std::net::TcpListener::bind(("127.0.0.1", port)).expect("the port is still taken"));

        assert!(set(&atlas, local(true, port)).await.running, "starts again on the same port");
        atlas.shutdown().await;
        assert!(!atlas.remote.status().await.running);
    }

    #[tokio::test]
    async fn a_busy_port_is_reported_not_thrown() {
        let atlas = Atlas::start(AppConfig::default(), None, None, no_updates()).unwrap();
        let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = taken.local_addr().unwrap().port();
        let s = set(&atlas, local(true, port)).await;
        assert!(!s.running, "{s:?}");
        // Windows lets a second socket share the port unless it is exclusive: either way it's not ours
        if Os::CURRENT != Os::Windows {
            assert_eq!(s.failure, Some(RemoteFailure::PortInUse));
            assert!(s.error.unwrap().contains(&port.to_string()));
        }
    }

    #[tokio::test]
    async fn the_choice_is_saved() {
        let dir = std::env::temp_dir().join(format!("swatlas-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("swatlas.toml");
        let atlas = Atlas::start(AppConfig::default(), Some(path.clone()), None, no_updates()).unwrap();
        // not enabled: nothing listens on every interface during the test
        let c = RemoteChoice { enabled: false, bind: "0.0.0.0".into(), port: 8090 };
        let s = set(&atlas, c.clone()).await;
        assert_eq!((s.choice.clone(), s.scope), (c, Scope::Network));
        assert_eq!(s.config_path, path.display().to_string());
        assert_eq!(AppConfig::load(&path).unwrap().server.choice(), s.choice);

        let bad = RemoteChoice { port: 0, ..s.choice };
        assert_eq!(atlas.call(Call::SetRemote(bad)).await.unwrap_err().kind, ErrorKind::Invalid);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn swatlas_server_always_serves() {
        let atlas = Atlas::start(AppConfig::default(), None, None, no_updates()).unwrap();
        let port = free_port();
        atlas.remote.serve_headless(&atlas, local(false, port)).await.unwrap();
        let s = atlas.remote.status().await;
        assert!(s.headless && s.running && s.choice.enabled, "{s:?}");
        let e = atlas.call(Call::SetRemote(local(false, port))).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Invalid);
        assert!(atlas.remote.status().await.running);
        atlas.shutdown().await;
    }
}
