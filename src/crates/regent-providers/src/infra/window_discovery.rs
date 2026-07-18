//! Best-effort LIVE context-window discovery, layered ABOVE the static
//! `model_windows` table so a freshly-released or unlisted model still gets
//! its real window instead of falling through to `None`. Only two upstreams
//! reliably expose window metadata over plain HTTP, so those are the only
//! two fetch paths implemented:
//!
//! - **OpenRouter**: `GET {base}/models` returns the whole catalog
//!   (`data[].id` + `data[].context_length`) in one shot — cached for the
//!   process on first fetch.
//! - **Anthropic**: `GET {base}/v1/models/{id}` returns `max_input_tokens`,
//!   which is exactly the number compaction preflight sizes against.
//!
//! Ollama and generic OpenAI-compatible endpoints are deliberately skipped —
//! there's no reliable metadata endpoint across that zoo of hosts, so those
//! stay on the static table / the user's `context.windows` config override.
//!
//! Fetches are spawned once at provider construction (only when a tokio
//! runtime is already running — tests that construct providers outside one
//! just skip discovery) and never block or fail construction or a chat call:
//! a slow or failing fetch leaves the cache empty and callers fall back to
//! `model_windows::window_for_model`.

use reqwest::Client;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

fn cache() -> &'static Mutex<HashMap<String, u32>> {
    static CACHE: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_key(endpoint_key: &str, model: &str) -> String {
    format!("{endpoint_key}::{model}")
}

fn store(endpoint_key: &str, model: &str, window: u32) {
    if let Ok(mut map) = cache().lock() {
        map.insert(cache_key(endpoint_key, model), window);
    }
}

/// A previously-discovered window for `model` behind `endpoint_key` (its
/// provider's base URL), if a background fetch already landed one. `None`
/// before the fetch completes, if discovery isn't supported, or if it failed
/// — callers fall back to the static table in that case.
#[must_use]
pub(crate) fn discovered_window(endpoint_key: &str, model: &str) -> Option<u32> {
    cache()
        .lock()
        .ok()?
        .get(&cache_key(endpoint_key, model))
        .copied()
}

/// Parses OpenRouter's `/models` catalog into `(id, context_length)` pairs,
/// dropping entries without a window. Pure and fixture-testable.
fn parse_openrouter_catalog(body: &str) -> Vec<(String, u32)> {
    #[derive(serde::Deserialize)]
    struct Catalog {
        data: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        id: String,
        context_length: Option<u32>,
    }
    serde_json::from_str::<Catalog>(body)
        .map(|c| {
            c.data
                .into_iter()
                .filter_map(|e| e.context_length.map(|w| (e.id, w)))
                .collect()
        })
        .unwrap_or_default()
}

/// Parses an Anthropic `/v1/models/{id}` response into its input window.
/// Pure and fixture-testable.
fn parse_anthropic_model(body: &str) -> Option<u32> {
    #[derive(serde::Deserialize)]
    struct ModelInfo {
        max_input_tokens: Option<u32>,
    }
    serde_json::from_str::<ModelInfo>(body)
        .ok()?
        .max_input_tokens
}

/// Returns whether the catalog was fetched and parsed — a failed fetch must
/// NOT mark the endpoint done, or one offline startup would kill discovery
/// for the process lifetime.
async fn fetch_openrouter(client: &Client, base_url: &str) -> bool {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let body = match client.get(&url).timeout(FETCH_TIMEOUT).send().await {
        Ok(resp) => resp.text().await,
        Err(e) => {
            tracing::debug!(%e, "openrouter window discovery: request failed");
            return false;
        }
    };
    match body {
        Ok(body) => {
            let entries = parse_openrouter_catalog(&body);
            let fetched = !entries.is_empty();
            for (id, window) in entries {
                store(base_url, &id, window);
            }
            fetched
        }
        Err(e) => {
            tracing::debug!(%e, "openrouter window discovery: body read failed");
            false
        }
    }
}

async fn fetch_anthropic(
    client: &Client,
    base_url: &str,
    api_key: &str,
    version: &str,
    model: &str,
) {
    let url = format!("{}/v1/models/{model}", base_url.trim_end_matches('/'));
    let body = match client
        .get(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", version)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
    {
        Ok(resp) => resp.text().await,
        Err(e) => {
            tracing::debug!(%e, "anthropic window discovery: request failed");
            return;
        }
    };
    match body {
        Ok(body) => {
            if let Some(window) = parse_anthropic_model(&body) {
                store(base_url, model, window);
            }
        }
        Err(e) => tracing::debug!(%e, "anthropic window discovery: body read failed"),
    }
}

/// Sentinel model name marking "this endpoint's catalog was already fetched"
/// — providers are constructed per SESSION, so without it every new session
/// would re-pull the whole OpenRouter catalog.
const CATALOG_FETCHED: &str = "__catalog__";

/// Spawns a background fetch of OpenRouter's whole catalog for `base_url`
/// (one request caches every model — and only the FIRST construction per
/// endpoint fetches at all). No-op if there's no tokio runtime currently
/// entered (e.g. a provider built synchronously in a unit test). Builds its
/// own `Client` AFTER the cache-hit guard — providers are constructed per
/// session, and the steady state must not pay a TLS/pool setup per session.
pub(crate) fn spawn_openrouter_discovery(base_url: String) {
    if discovered_window(&base_url, CATALOG_FETCHED).is_some() {
        return;
    }
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            if fetch_openrouter(&Client::new(), &base_url).await {
                store(&base_url, CATALOG_FETCHED, 1);
            }
        });
    }
}

/// Spawns a background fetch of `model`'s window from Anthropic's model
/// metadata endpoint — skipped once a window for `(endpoint, model)` is
/// cached. Same no-runtime no-op as `spawn_openrouter_discovery`.
pub(crate) fn spawn_anthropic_discovery(
    client: Client,
    base_url: String,
    api_key: String,
    version: String,
    model: String,
) {
    if discovered_window(&base_url, &model).is_some() {
        return;
    }
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            fetch_anthropic(&client, &base_url, &api_key, &version, &model).await
        });
    }
}

#[cfg(test)]
#[path = "tests/window_discovery_tests.rs"]
mod tests;
