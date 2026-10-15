//! HTTPS GET for the update check and the NET band: one client, one User-Agent
//! (`swatlas/<version>`, which radio-browser.info asks its clients to send).

use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

/// Waiting this long for a server to answer at all gives up.
#[cfg(feature = "update-check")]
const CONNECT: Duration = Duration::from_secs(10);

#[cfg(feature = "update-check")]
fn client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    HTTP.get_or_init(|| {
        // reqwest is built without a crypto provider of its own: use ring, like Tauri's updater
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .user_agent(concat!("swatlas/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(CONNECT)
            .build()
            .expect("HTTP client")
    })
}

#[cfg(feature = "update-check")]
pub async fn get(url: &str, timeout: Duration) -> Result<String, String> {
    async { client().get(url).timeout(timeout).send().await?.error_for_status()?.text().await }
        .await
        .map_err(|e| format!("could not read {url}: {e}"))
}

#[cfg(not(feature = "update-check"))]
pub async fn get(_: &str, _: Duration) -> Result<String, String> {
    Err("built without HTTPS".into())
}

/// Read an endless response (a radio stream) into `tx` as it arrives. Returns `Ok` once `tx`'s
/// receiver is gone (the listener stopped), an error if the server fails, the stream ends, or
/// nothing arrives for `stall`.
#[cfg(feature = "update-check")]
pub async fn stream(url: &str, stall: Duration, tx: mpsc::Sender<Bytes>) -> Result<(), String> {
    let send = tokio::time::timeout(stall, client().get(url).send());
    let mut resp = match send.await {
        Ok(r) => r.and_then(|r| r.error_for_status()).map_err(|e| format!("could not open the stream: {e}"))?,
        Err(_) => return Err("the stream did not answer".into()),
    };
    loop {
        let chunk = tokio::select! {
            _ = tx.closed() => return Ok(()),
            c = tokio::time::timeout(stall, resp.chunk()) => c,
        };
        match chunk {
            Ok(Ok(Some(b))) => {
                if tx.send(b).await.is_err() {
                    return Ok(());
                }
            }
            Ok(Ok(None)) => return Err("the stream ended".into()),
            Ok(Err(e)) => return Err(format!("the stream broke off: {e}")),
            Err(_) => return Err(format!("no audio from the stream for {} s", stall.as_secs())),
        }
    }
}

#[cfg(not(feature = "update-check"))]
pub async fn stream(_: &str, _: Duration, _: mpsc::Sender<Bytes>) -> Result<(), String> {
    Err("built without HTTPS".into())
}
