//! The NET band's IO: station searches on the radio-browser.info directory, off unless the
//! listener turns internet radio on. The directory asks its clients to find its servers from its
//! server list and to move on to the next one when a server fails; a short cache keeps typing and
//! paging from asking it the same thing twice. Nothing here touches the rig or the listening
//! history.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atlas_core::api::{ApiError, ErrorKind};
use atlas_core::net::{page, parse_servers, search_path, NetPage, NetQuery, SERVERS_URL};

/// A search answer is reused this long.
const PAGE_TTL: Duration = Duration::from_secs(5 * 60);
/// Answers kept at most; the cache starts over past this.
const PAGE_CACHE: usize = 200;
/// The server list is read again after this.
const SERVERS_TTL: Duration = Duration::from_secs(6 * 3600);
/// Used when the server list can't be read: any server answers there.
const FALLBACK_SERVER: &str = "all.api.radio-browser.info";
const TIMEOUT: Duration = Duration::from_secs(10);

pub type Fetch = Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> + Send + Sync>;
type SharedFetch = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> + Send + Sync>;

pub struct NetService {
    enabled: AtomicBool,
    fetch: RwLock<SharedFetch>,
    servers: Mutex<Option<(Instant, Vec<String>)>>,
    pages: Mutex<HashMap<NetQuery, (Instant, NetPage)>>,
}

impl NetService {
    pub fn new(enabled: bool) -> Self {
        Self::with_fetch(enabled, Box::new(|url| Box::pin(async move { crate::http::get(&url, TIMEOUT).await })))
    }

    /// With another way to read a URL (tests).
    pub fn with_fetch(enabled: bool, fetch: Fetch) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            fetch: RwLock::new(Arc::from(fetch)),
            servers: Mutex::new(None),
            pages: Mutex::new(HashMap::new()),
        }
    }

    /// Read URLs another way from now on (tests).
    #[cfg(test)]
    pub(crate) fn replace_fetch(&self, fetch: Fetch) {
        *self.fetch.write().expect("net fetch") = Arc::from(fetch);
    }

    fn get(&self, url: String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let f = self.fetch.read().expect("net fetch").clone();
        f(url)
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        if !on {
            self.pages.lock().expect("net pages").clear();
        }
    }

    pub async fn search(&self, q: &NetQuery) -> Result<NetPage, ApiError> {
        if !self.enabled() {
            return Err(ApiError { kind: ErrorKind::Conflict, message: "Internet radio is off".into() });
        }
        if let Some((at, p)) = self.pages.lock().expect("net pages").get(q) {
            if at.elapsed() < PAGE_TTL {
                return Ok(p.clone());
            }
        }
        let Some(path) = search_path(q) else { return Ok(NetPage::default()) };
        let mut last = String::new();
        for server in self.servers().await {
            match self.get(format!("https://{server}{path}")).await.and_then(|json| page(&json, q)) {
                Ok(p) => {
                    let mut pages = self.pages.lock().expect("net pages");
                    if pages.len() >= PAGE_CACHE {
                        pages.clear();
                    }
                    pages.insert(q.clone(), (Instant::now(), p.clone()));
                    return Ok(p);
                }
                Err(e) => {
                    tracing::warn!("radio-browser {server}: {e}");
                    last = e;
                }
            }
        }
        Err(ApiError::internal(format!("Internet radio is unreachable: {last}")))
    }

    /// The directory's servers, starting at a different one each time so the load spreads;
    /// the fallback name when the list can't be read.
    async fn servers(&self) -> Vec<String> {
        let cached = self
            .servers
            .lock()
            .expect("net servers")
            .as_ref()
            .and_then(|(at, list)| (at.elapsed() < SERVERS_TTL && !list.is_empty()).then(|| list.clone()));
        let mut list = match cached {
            Some(l) => l,
            None => match self.get(SERVERS_URL.to_string()).await.and_then(|j| parse_servers(&j)) {
                Ok(l) if !l.is_empty() => {
                    *self.servers.lock().expect("net servers") = Some((Instant::now(), l.clone()));
                    l
                }
                Ok(_) => vec![FALLBACK_SERVER.to_string()],
                Err(e) => {
                    tracing::warn!("radio-browser server list: {e}");
                    vec![FALLBACK_SERVER.to_string()]
                }
            },
        };
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let n = list.len();
        list.rotate_left(nanos as usize % n);
        list
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const ROW: &str = r#"{"stationuuid":"u1","name":"La 100 - 99.9 FM","countrycode":"AR",
        "language":"spanish","tags":"pop","codec":"MP3","bitrate":128,"hls":0,"lastcheckok":1,"votes":3}"#;

    /// A directory with two servers, the first of them down; every URL asked is kept.
    pub(crate) fn fake(asked: Arc<Mutex<Vec<String>>>) -> Fetch {
        Box::new(move |url| {
            asked.lock().unwrap().push(url.clone());
            Box::pin(async move {
                if url == SERVERS_URL {
                    Ok(r#"[{"name":"down.api.radio-browser.info"},{"name":"up.api.radio-browser.info"}]"#.into())
                } else if url.starts_with("https://up.") {
                    Ok(format!("[{ROW}]"))
                } else {
                    Err("connection refused".into())
                }
            })
        })
    }

    #[tokio::test]
    async fn off_until_turned_on() {
        let asked = Arc::new(Mutex::new(vec![]));
        let net = NetService::with_fetch(false, fake(asked.clone()));
        let e = net.search(&NetQuery::default()).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Conflict);
        assert!(asked.lock().unwrap().is_empty(), "nothing is sent while off");
        net.set_enabled(true);
        assert!(net.search(&NetQuery::default()).await.is_ok());
    }

    #[tokio::test]
    async fn moves_on_to_the_next_server_and_caches() {
        let asked = Arc::new(Mutex::new(vec![]));
        let net = NetService::with_fetch(true, fake(asked.clone()));
        let q = NetQuery { country: Some("ARG".into()), ..Default::default() };
        let p = net.search(&q).await.unwrap();
        assert_eq!((p.items.len(), p.items[0].itu.as_deref(), p.more), (1, Some("ARG"), false));
        let n = asked.lock().unwrap().len();
        assert!((2..=3).contains(&n), "the server list, then one or both servers: {n}");
        assert!(asked.lock().unwrap().iter().all(|u| u == SERVERS_URL || u.contains("countrycode=AR")));
        // the same query again: from the cache
        net.search(&q).await.unwrap();
        assert_eq!(asked.lock().unwrap().len(), n);
        // a country the directory has no code for: nothing asked, nothing found
        let none = net.search(&NetQuery { country: Some("ALS".into()), ..Default::default() }).await.unwrap();
        assert!(none.items.is_empty());
        assert_eq!(asked.lock().unwrap().len(), n);
    }

    /// The real directory: `cargo test -p atlas-server -- --ignored live_directory`.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn live_directory() {
        let net = NetService::new(true);
        let q = NetQuery { q: "radio".into(), country: Some("ARG".into()), limit: 10, ..Default::default() };
        let p = net.search(&q).await.unwrap();
        assert_eq!(p.items.len(), 10, "{p:?}");
        assert!(p.more);
        assert!(p.items.iter().all(|s| s.itu.as_deref() == Some("ARG") && !s.uuid.is_empty()), "{p:?}");
        assert!(p.items.iter().all(|s| s.name == s.name.trim()));
    }

    #[tokio::test]
    async fn unreachable_directory() {
        let net = NetService::with_fetch(true, Box::new(|_| Box::pin(async { Err("offline".to_string()) })));
        let e = net.search(&NetQuery::default()).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::Internal);
        assert!(e.message.contains("offline"), "{}", e.message);
    }
}
