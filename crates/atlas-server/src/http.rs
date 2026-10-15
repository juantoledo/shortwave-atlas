//! HTTPS GET for the update check and the NET band: one client, one User-Agent
//! (`swatlas/<version>`, which radio-browser.info asks its clients to send).

use std::time::Duration;

#[cfg(feature = "update-check")]
pub async fn get(url: &str, timeout: Duration) -> Result<String, String> {
    use std::sync::OnceLock;
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    let http = HTTP.get_or_init(|| {
        // reqwest is built without a crypto provider of its own: use ring, like Tauri's updater
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .user_agent(concat!("swatlas/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client")
    });
    async { http.get(url).timeout(timeout).send().await?.error_for_status()?.text().await }
        .await
        .map_err(|e| format!("could not read {url}: {e}"))
}

#[cfg(not(feature = "update-check"))]
pub async fn get(_: &str, _: Duration) -> Result<String, String> {
    Err("built without HTTPS".into())
}
