//! Pluggable web-search providers — the search analog of the gateway's
//! per-platform adapters. Each provider's `build_request` + `parse_response`
//! are **pure** (no I/O), so they unit-test without a key or network; the
//! `web_search` tool executes the request descriptor generically (mirrors the
//! gateway's `SendRequest` build/execute split). Add a provider = one file
//! here + one arm in `provider_from_name`.

use serde_json::Value;

pub mod brave;
pub mod duckduckgo;
pub mod exa;
pub mod google_cse;
pub mod serpapi;
pub mod tavily;

/// One web result, normalized across providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// A provider-built HTTP request descriptor. Query params are left unencoded —
/// the executor encodes them — so providers stay pure and platform-agnostic.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub method: Method,
    pub url: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
}

/// A web-search backend. Implementations are stateless unit structs.
pub trait SearchProvider: Send + Sync {
    fn name(&self) -> &str;
    /// Env var holding this provider's API key, or `None` if keyless.
    fn key_env(&self) -> Option<&'static str>;
    /// Build the HTTP request for `query` (pure — no network, no env beyond
    /// secondary config like a CSE id).
    fn build_request(&self, query: &str, api_key: Option<&str>, count: usize) -> SearchRequest;
    /// Parse the provider's JSON response body into normalized results (pure).
    fn parse_response(&self, body: &[u8]) -> Result<Vec<SearchResult>, String>;
}

/// Map a provider name to its adapter. Aliases included for convenience.
#[must_use]
pub fn provider_from_name(name: &str) -> Option<Box<dyn SearchProvider>> {
    match name.trim().to_lowercase().as_str() {
        "brave" => Some(Box::new(brave::Brave)),
        "tavily" => Some(Box::new(tavily::Tavily)),
        "serpapi" | "serp" => Some(Box::new(serpapi::SerpApi)),
        "exa" => Some(Box::new(exa::Exa)),
        "google" | "google_cse" | "cse" => Some(Box::new(google_cse::GoogleCse)),
        "duckduckgo" | "ddg" => Some(Box::new(duckduckgo::DuckDuckGo)),
        _ => None,
    }
}

/// Keyed providers, in preference order, with the env that activates each —
/// used for auto-selection when no provider is named explicitly.
const KEYED: &[(&str, &str)] = &[
    ("brave", "BRAVE_API_KEY"),
    ("tavily", "TAVILY_API_KEY"),
    ("serpapi", "SERPAPI_API_KEY"),
    ("exa", "EXA_API_KEY"),
    ("google_cse", "GOOGLE_CSE_API_KEY"),
];

/// The active provider. An explicit `REGENT_SEARCH_PROVIDER` wins; otherwise we
/// auto-select the first keyed provider whose key is present (so saving e.g.
/// `TAVILY_API_KEY` is enough — real ranked results, the ≥12-source policy
/// holds). Falls back to the keyless DuckDuckGo when nothing is configured.
#[must_use]
pub fn provider_from_env() -> Box<dyn SearchProvider> {
    if let Some(p) = std::env::var("REGENT_SEARCH_PROVIDER").ok().and_then(|n| provider_from_name(&n))
    {
        return p;
    }
    for (name, key_env) in KEYED {
        if std::env::var(key_env).is_ok_and(|v| !v.trim().is_empty())
            && let Some(p) = provider_from_name(name)
        {
            return p;
        }
    }
    Box::new(duckduckgo::DuckDuckGo)
}

/// Resolve the API key for a provider: its dedicated env first, then the
/// generic `REGENT_SEARCH_API_KEY`. `None` for keyless providers.
#[must_use]
pub fn resolve_key(provider: &dyn SearchProvider) -> Option<String> {
    let dedicated = provider.key_env().and_then(|e| std::env::var(e).ok());
    dedicated
        .or_else(|| std::env::var("REGENT_SEARCH_API_KEY").ok())
        .filter(|k| !k.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_aliases_map_to_providers() {
        for name in ["brave", "tavily", "serpapi", "serp", "exa", "google_cse", "cse", "ddg"] {
            assert!(provider_from_name(name).is_some(), "{name} should resolve");
        }
        assert!(provider_from_name("nope").is_none());
        // Every auto-select candidate must be a real provider name.
        for (name, _) in KEYED {
            assert!(provider_from_name(name).is_some(), "KEYED name {name} must resolve");
        }
    }
}

/// Shared helper: pull `[{title,url,snippet}]` from a JSON array at `path`,
/// reading each field via the given keys (snippet optional).
pub(crate) fn map_results(
    array: &[Value],
    title_key: &str,
    url_key: &str,
    snippet_key: &str,
) -> Vec<SearchResult> {
    array
        .iter()
        .filter_map(|r| {
            Some(SearchResult {
                title: r.get(title_key)?.as_str()?.to_owned(),
                url: r.get(url_key)?.as_str()?.to_owned(),
                snippet: r
                    .get(snippet_key)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
        })
        .collect()
}
