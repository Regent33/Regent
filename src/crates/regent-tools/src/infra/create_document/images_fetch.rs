//! Network + cache side of image sourcing: the keyless Openverse search, the
//! direct download, and the per-document fetch cache.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

const OPENVERSE_ENDPOINT: &str = "https://api.openverse.org/v1/images/";
const TIMEOUT_SECS: u64 = 20;
const USER_AGENT: &str = "Regent/0.1 (document images)";

/// How many candidates a search returns.
///
/// It used to ask for one. Openverse indexes mostly Flickr, and a network that
/// cannot reach `live.staticflickr.com` — measured on the owner's, where the
/// Openverse API itself answers fine and Unsplash downloads fine — therefore got
/// NO image, ever, from any query. One unreachable host must not be the whole
/// answer, so the caller walks the list until something downloads.
const CANDIDATES: usize = 8;

/// The provider to fall back to when nothing from the default search will
/// download. Openverse's index is ~536M Flickr images against ~88M Wikimedia,
/// so an unrestricted search returns Flickr almost exclusively — and where
/// Flickr's CDN is unreachable that means no image at all. Wikimedia serves
/// from `upload.wikimedia.org`, an entirely different host.
pub const FALLBACK_SOURCE: &str = "wikimedia";

/// Commercially-usable image URLs for `query`, best first. `source` restricts
/// the provider (`None` = any). An empty vec means the search ran and matched
/// nothing; `Err` is a transport/parse failure.
pub async fn search(query: &str, source: Option<&str>) -> Result<Vec<String>, String> {
    let page_size = CANDIDATES.to_string();
    let mut params: Vec<(&str, &str)> = vec![
        ("q", query),
        ("page_size", &page_size),
        ("license_type", "commercial"),
    ];
    if let Some(source) = source {
        params.push(("source", source));
    }
    let url = reqwest::Url::parse_with_params(OPENVERSE_ENDPOINT, &params)
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
    Ok(result_urls(&body))
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

/// Every result's image URL, in the order the search ranked them. Pure, so the
/// extraction is testable without a network round trip.
fn result_urls(body: &Value) -> Vec<String> {
    body["results"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .filter_map(|item| item["url"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
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
    /// Every candidate, in rank order — not just the top one. Taking only the
    /// first meant a single unreachable image host produced no picture at all.
    fn result_urls_keeps_every_candidate_in_order() {
        let hit = json!({"results": [{"url": "https://x/y.jpg"}, {"url": "https://a/b.jpg"}]});
        assert_eq!(result_urls(&hit), ["https://x/y.jpg", "https://a/b.jpg"]);
        // A result missing its url is skipped, not fatal — the rest still count.
        let ragged = json!({"results": [{"title": "no url"}, {"url": "https://a/b.jpg"}]});
        assert_eq!(result_urls(&ragged), ["https://a/b.jpg"]);
        assert!(result_urls(&json!({"results": []})).is_empty());
        assert!(result_urls(&json!({})).is_empty());
    }
}
