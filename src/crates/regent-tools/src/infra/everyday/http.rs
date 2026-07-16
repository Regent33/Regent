//! Shared HTTP plumbing for the everyday network tools (weather, sun_moon,
//! dictionary, convert, geocoding). One process-wide client — connection
//! reuse instead of a fresh TLS handshake per tool call — with a bounded
//! timeout, and one fetch shape whose every failure names the service.

use std::sync::OnceLock;
use std::time::Duration;

const TIMEOUT_SECS: u64 = 15;

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .user_agent("regent/0.1 (+https://github.com/regent)")
            .build()
            // Static, valid options — cannot fail at runtime.
            .expect("static reqwest client options")
    })
}

/// GET `url`; returns `(status, body bytes)`. Transport failures name
/// `service`. Callers that need status-specific handling (dictionary's 404)
/// use this; everyone else wants [`fetch_ok`].
pub(super) async fn fetch(service: &str, url: &str) -> Result<(u16, Vec<u8>), String> {
    let resp = client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("{service} request failed: {e}"))?;
    let status = resp.status().as_u16();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("{service} read failed: {e}"))?;
    Ok((status, bytes.to_vec()))
}

/// GET `url`; 2xx → body bytes, anything else → an error carrying the status
/// and a body snippet — never a silent empty result.
pub(super) async fn fetch_ok(service: &str, url: &str) -> Result<Vec<u8>, String> {
    let (status, bytes) = fetch(service, url).await?;
    if !(200..300).contains(&status) {
        let snippet: String = String::from_utf8_lossy(&bytes).chars().take(300).collect();
        return Err(format!("{service} HTTP {status}: {snippet}"));
    }
    Ok(bytes)
}
