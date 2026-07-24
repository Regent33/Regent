//! One conditional GET for the manifest: an ETag replay plus a hard body cap.
//!
//! The URL is built by the caller from a fixed repo slug — this function never
//! reads a URL out of manifest data and never follows a redirect chain to an
//! arbitrary host beyond what reqwest's default (same-policy) client allows.
//! Every outcome is a small verdict; it never panics.

use super::model::MAX_MANIFEST_BYTES;

/// Result of a single conditional GET.
pub enum FetchOutcome {
    /// Server returned `304 Not Modified` — the cached manifest is still current.
    NotModified,
    /// A fresh, already size-capped body plus the new ETag (if the server sent one).
    Fetched { body: Vec<u8>, etag: Option<String> },
    /// Network error, non-success status, or an oversized body — reason is bounded.
    Failed(String),
}

/// Perform the conditional GET. When `etag` is present it is sent as
/// `If-None-Match`, letting the server answer `304` and save the transfer.
pub async fn conditional_get(
    client: &reqwest::Client,
    url: &str,
    etag: Option<&str>,
) -> FetchOutcome {
    let mut req = client.get(url);
    if let Some(tag) = etag {
        req = req.header(reqwest::header::IF_NONE_MATCH, tag);
    }
    let mut resp = match req.send().await {
        Ok(response) => response,
        Err(error) => return FetchOutcome::Failed(short(&error.to_string())),
    };
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return FetchOutcome::NotModified;
    }
    if !resp.status().is_success() {
        return FetchOutcome::Failed(format!("HTTP {}", resp.status().as_u16()));
    }
    if let Some(len) = resp.content_length()
        && len as usize > MAX_MANIFEST_BYTES
    {
        return FetchOutcome::Failed(format!("manifest too large: {len} bytes"));
    }
    let new_etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let mut body = Vec::with_capacity(
        resp.content_length()
            .unwrap_or(0)
            .min(MAX_MANIFEST_BYTES as u64) as usize,
    );
    loop {
        let chunk = match resp.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => return FetchOutcome::Failed(short(&error.to_string())),
        };
        if body.len().saturating_add(chunk.len()) > MAX_MANIFEST_BYTES {
            return FetchOutcome::Failed(format!(
                "manifest too large: more than {MAX_MANIFEST_BYTES} bytes"
            ));
        }
        body.extend_from_slice(&chunk);
    }
    FetchOutcome::Fetched {
        body,
        etag: new_etag,
    }
}

/// Bound an error string so a hostile server can't blow up the diagnostic.
fn short(s: &str) -> String {
    s.chars().take(200).collect()
}
