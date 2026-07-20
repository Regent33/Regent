//! Network + cache side of image sourcing: the keyless Openverse search, the
//! direct download, and the per-document fetch cache.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

const OPENVERSE_ENDPOINT: &str = "https://api.openverse.org/v1/images/";
const TIMEOUT_SECS: u64 = 20;
const USER_AGENT: &str = "Regent/0.1 (document images)";

/// Find one commercially-usable image URL for `query`. `Ok(None)` = the search
/// ran but matched nothing; `Err` = transport/parse failure.
pub async fn search(query: &str) -> Result<Option<String>, String> {
    let url = reqwest::Url::parse_with_params(
        OPENVERSE_ENDPOINT,
        &[
            ("q", query),
            ("page_size", "1"),
            ("license_type", "commercial"),
        ],
    )
    .map_err(|error| format!("bad image search url: {error}"))?;
    let body: Value = client()?
        .get(url)
        .send()
        .await
        .map_err(|error| format!("image search request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("image search HTTP error: {error}"))?
        .json()
        .await
        .map_err(|error| format!("image search returned a bad response: {error}"))?;
    Ok(first_url(&body))
}

/// Download raw image bytes from a direct URL.
pub async fn download(url: &str) -> Result<Vec<u8>, String> {
    let bytes = client()?
        .get(url)
        .send()
        .await
        .map_err(|error| format!("image download failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("image download HTTP error: {error}"))?
        .bytes()
        .await
        .map_err(|error| format!("image download read failed: {error}"))?;
    Ok(bytes.to_vec())
}

/// The first result's image URL, if the response carried one. Pure, so the
/// extraction is testable without a network round trip.
fn first_url(body: &Value) -> Option<String> {
    body["results"][0]["url"].as_str().map(str::to_owned)
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| error.to_string())
}

/// A stable cache file path under `<deck-folder>/images/` for a url/query key, or
/// `None` when there's no folder to cache in. The filename hashes the key so the
/// same query maps to the same file across edits.
/// ponytail: SipHash via DefaultHasher — deterministic within a build, which is
/// all a per-document image cache needs; swap for a content hash if cross-build
/// stability is ever required.
pub async fn cache_path(cache_dir: Option<&Path>, key: &str) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let dir = cache_dir?.join("images");
    tokio::fs::create_dir_all(&dir).await.ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    Some(dir.join(format!("{:016x}.img", hasher.finish())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn first_url_extracts_the_top_result_or_none() {
        let hit = json!({"results": [{"url": "https://x/y.jpg"}, {"url": "https://a/b.jpg"}]});
        assert_eq!(first_url(&hit).as_deref(), Some("https://x/y.jpg"));
        assert_eq!(first_url(&json!({"results": []})), None);
        assert_eq!(first_url(&json!({})), None);
    }
}
