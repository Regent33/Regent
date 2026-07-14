//! The `web_search` + `web_fetch` tools. `web_search` selects a provider from
//! the environment (see `search_providers`), executes its request descriptor
//! generically, and returns normalized results; `web_fetch` retrieves a URL and
//! strips it to readable text. Network is the only impurity here — providers
//! and parsing stay pure and unit-tested in their own modules.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use crate::infra::net;
use crate::infra::search_providers::{Method, SearchProvider, provider_from_env, resolve_key};
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::time::Duration;

// At least 12 sources per search (policy: breadth + verifiability); the count is
// floored here so it holds even if the model asks for fewer.
const MIN_COUNT: usize = 12;
const MAX_COUNT: usize = 20;
const HTTP_TIMEOUT_SECS: u64 = 20;
const FETCH_MAX_CHARS: usize = 12_000;
const FETCH_MAX_BYTES: usize = 5_000_000; // 5 MB download cap (memory-DoS guard)

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent("regent/0.1 (+https://github.com/regent)")
        .build()
        .unwrap_or_default()
}

// ── web_search ──────────────────────────────────────────────────────────────

#[must_use]
pub fn search_definition() -> ToolDefinition {
    ToolDefinition {
        name: "web_search".into(),
        description:
            "Search the live web and return ranked results (title, url, snippet). Returns \
                      at least 12 sources. Use it for anything beyond your training data, then \
                      web_fetch a result for details. ALWAYS cite the sources you used in your \
                      answer — a numbered list of the result links (references)."
                .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "The search query."},
                "count": {"type": "integer", "description": "Result count (min 12, max 20)."}
            },
            "required": ["query"]
        }),
        toolset: "web".into(),
    }
}

pub struct WebSearchTool;

#[async_trait]
impl ToolExecutor for WebSearchTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(query) = args
            .get("query")
            .and_then(Value::as_str)
            .filter(|q| !q.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: query"));
        };
        // Floor at MIN_COUNT so every search pulls at least 12 sources.
        let count = args
            .get("count")
            .and_then(Value::as_u64)
            .map_or(MIN_COUNT, |n| (n as usize).clamp(MIN_COUNT, MAX_COUNT));

        let provider = provider_from_env();
        let key = resolve_key(provider.as_ref());
        if provider.key_env().is_some() && key.is_none() {
            return Ok(tool_error_json(format!(
                "web_search: provider '{}' needs an API key — set {} (or REGENT_SEARCH_API_KEY)",
                provider.name(),
                provider.key_env().unwrap_or("REGENT_SEARCH_API_KEY"),
            )));
        }
        Ok(run_search(provider.as_ref(), query, key.as_deref(), count).await)
    }
}

async fn run_search(
    provider: &dyn SearchProvider,
    query: &str,
    key: Option<&str>,
    count: usize,
) -> String {
    let req = provider.build_request(query, key, count);
    let url = if req.query.is_empty() {
        reqwest::Url::parse(&req.url)
    } else {
        reqwest::Url::parse_with_params(&req.url, req.query.iter().map(|(k, v)| (k, v)))
    };
    let url = match url {
        Ok(u) => u,
        Err(e) => return tool_error_json(format!("web_search: bad url: {e}")),
    };
    let client = http_client();
    let mut builder = match req.method {
        Method::Get => client.get(url),
        Method::Post => client.post(url),
    };
    for (k, v) in &req.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if let Some(body) = &req.body {
        builder = builder.json(body);
    }
    let resp = match builder.send().await {
        Ok(r) => r,
        Err(e) => return tool_error_json(format!("web_search request failed: {e}")),
    };
    let status = resp.status();
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return tool_error_json(format!("web_search read failed: {e}")),
    };
    if !status.is_success() {
        let snippet: String = String::from_utf8_lossy(&bytes).chars().take(300).collect();
        return tool_error_json(format!("web_search HTTP {}: {snippet}", status.as_u16()));
    }
    match provider.parse_response(&bytes) {
        Ok(results) => json!({
            "provider": provider.name(),
            "query": query,
            "results": results.iter().map(|r| json!({
                "title": r.title, "url": r.url, "snippet": r.snippet,
            })).collect::<Vec<_>>(),
        })
        .to_string(),
        Err(e) => tool_error_json(format!("web_search parse failed: {e}")),
    }
}

// ── web_fetch ─────────────────────────────────────────────────────────────--

#[cfg(test)]
#[path = "web_search_tests.rs"]
mod tests;

#[cfg(test)]
use web_fetch::html_to_text;
pub use web_fetch::{WebFetchTool, fetch_definition};

mod web_fetch;
