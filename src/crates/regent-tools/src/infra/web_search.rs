//! The `web_search` + `web_fetch` tools. `web_search` selects a provider from
//! the environment (see `search_providers`), executes its request descriptor
//! generically, and returns normalized results; `web_fetch` retrieves a URL and
//! strips it to readable text. Network is the only impurity here — providers
//! and parsing stay pure and unit-tested in their own modules.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use crate::infra::search_providers::{Method, SearchProvider, provider_from_env, resolve_key};
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regex::Regex;
use serde_json::{Value, json};
use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;

// At least 12 sources per search (policy: breadth + verifiability); the count is
// floored here so it holds even if the model asks for fewer.
const MIN_COUNT: usize = 12;
const MAX_COUNT: usize = 20;
const HTTP_TIMEOUT_SECS: u64 = 20;
const FETCH_MAX_CHARS: usize = 12_000;
const FETCH_MAX_BYTES: usize = 5_000_000; // 5 MB download cap (memory-DoS guard)
const MAX_REDIRECTS: usize = 5;

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
        description: "Search the live web and return ranked results (title, url, snippet). Returns \
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

#[must_use]
pub fn fetch_definition() -> ToolDefinition {
    ToolDefinition {
        name: "web_fetch".into(),
        description: "Fetch a URL and return its readable text (HTML stripped, truncated). Use \
                      after web_search to read a result, or to open a page/URL the user gives."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "The http(s) URL to fetch."},
                "max_chars": {"type": "integer", "description": "Truncate text (default 12000)."}
            },
            "required": ["url"]
        }),
        toolset: "web".into(),
    }
}

pub struct WebFetchTool;

#[async_trait]
impl ToolExecutor for WebFetchTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(url) = args
            .get("url")
            .and_then(Value::as_str)
            .filter(|u| !u.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: url"));
        };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Ok(tool_error_json(
                "web_fetch: url must start with http:// or https://",
            ));
        }
        let max_chars = args
            .get("max_chars")
            .and_then(Value::as_u64)
            .map_or(FETCH_MAX_CHARS, |n| n as usize);

        let (status, html) = match guarded_fetch(url).await {
            Ok(pair) => pair,
            Err(e) => return Ok(tool_error_json(e)),
        };
        let text = html_to_text(&html);
        let truncated = text.chars().count() > max_chars;
        let body: String = text.chars().take(max_chars).collect();
        Ok(json!({
            "url": url,
            "status": status,
            "truncated": truncated,
            "text": body,
        })
        .to_string())
    }
}

/// Fetch a URL with an SSRF guard: redirects are followed manually so every hop
/// is re-validated, each target host is DNS-resolved and rejected if it maps to
/// a non-public IP (loopback / private / link-local incl. cloud metadata
/// 169.254.169.254 / ULA / unspecified), and the body is read under a byte cap.
async fn guarded_fetch(url_str: &str) -> Result<(u16, String), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent("regent/0.1 (+https://github.com/regent)")
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("web_fetch client error: {e}"))?;

    let mut current = reqwest::Url::parse(url_str).map_err(|e| format!("bad url: {e}"))?;
    for _ in 0..=MAX_REDIRECTS {
        validate_public_url(&current).await?;
        let resp = client
            .get(current.clone())
            .send()
            .await
            .map_err(|e| format!("web_fetch failed: {e}"))?;
        let status = resp.status();
        if status.is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("web_fetch: redirect without a Location header")?;
            // Resolve relative redirects against the current URL, then re-validate.
            current = current
                .join(location)
                .map_err(|e| format!("bad redirect: {e}"))?;
            continue;
        }
        let body = read_capped(resp, FETCH_MAX_BYTES).await?;
        return Ok((status.as_u16(), body));
    }
    Err("web_fetch: too many redirects".into())
}

/// Reject anything but http(s) to a host that resolves only to public IPs.
async fn validate_public_url(url: &reqwest::Url) -> Result<(), String> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("web_fetch: only http(s) URLs are allowed".into());
    }
    let host = url.host_str().ok_or("web_fetch: url has no host")?;
    // An IP literal is checked directly; a hostname is resolved and *every*
    // address checked (defends against a name that points at an internal IP).
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs: Vec<IpAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![ip]
    } else {
        tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| format!("web_fetch: cannot resolve host: {e}"))?
            .map(|s| s.ip())
            .collect()
    };
    if addrs.is_empty() {
        return Err("web_fetch: host did not resolve".into());
    }
    if let Some(ip) = addrs.iter().find(|ip| is_blocked_ip(ip)) {
        return Err(format!(
            "web_fetch: refusing to fetch internal/private address ({ip})"
        ));
    }
    Ok(())
}

/// True if `ip` is not a public, routable address (SSRF denylist).
fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local() // 169.254.0.0/16 — incl. cloud metadata
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.octets()[0] == 0 // 0.0.0.0/8
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64) // 100.64/10 CGNAT
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
                || (v6.octets()[0] & 0xfe) == 0xfc       // unique-local fc00::/7
                // IPv4-mapped/compatible (::ffff:a.b.c.d) — check the embedded v4.
                || v6.to_ipv4().is_some_and(|m| is_blocked_ip(&IpAddr::V4(m)))
        }
    }
}

/// Read a response body, stopping at `max_bytes` (memory-DoS guard).
async fn read_capped(mut resp: reqwest::Response, max_bytes: usize) -> Result<String, String> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("web_fetch read failed: {e}"))?
    {
        buf.extend_from_slice(&chunk);
        if buf.len() >= max_bytes {
            buf.truncate(max_bytes);
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Strip `<script>`/`<style>` blocks and all tags, decode a few common
/// entities, and collapse whitespace. Good enough for feeding text to the model.
fn html_to_text(html: &str) -> String {
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    static TAGS: OnceLock<Regex> = OnceLock::new();
    static WS: OnceLock<Regex> = OnceLock::new();
    // No backreference (the `regex` crate lacks them); matching any of the three
    // closing tags is fine for stripping these blocks.
    let script = SCRIPT.get_or_init(|| {
        Regex::new(r"(?is)<(?:script|style|noscript)\b[^>]*>.*?</(?:script|style|noscript)>")
            .unwrap()
    });
    let tags = TAGS.get_or_init(|| Regex::new(r"(?s)<[^>]+>").unwrap());
    let ws = WS.get_or_init(|| Regex::new(r"\s+").unwrap());

    let no_script = script.replace_all(html, " ");
    let no_tags = tags.replace_all(&no_script, " ");
    let decoded = no_tags
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    ws.replace_all(&decoded, " ").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_to_text() {
        let html = "<html><head><style>x{}</style></head><body><p>Hello &amp; \
                    <b>world</b></p><script>bad()</script></body></html>";
        assert_eq!(html_to_text(html), "Hello & world");
    }

    #[test]
    fn ssrf_denylist_blocks_internal_addresses() {
        for ip in [
            "127.0.0.1",        // loopback
            "10.0.0.5",         // private
            "192.168.1.1",      // private
            "172.16.9.9",       // private
            "169.254.169.254",  // link-local — cloud metadata
            "0.0.0.0",          // unspecified
            "100.64.0.1",       // CGNAT
            "::1",              // IPv6 loopback
            "fe80::1",          // IPv6 link-local
            "fc00::1",          // IPv6 ULA
            "::ffff:127.0.0.1", // IPv4-mapped loopback
        ] {
            assert!(is_blocked_ip(&ip.parse().unwrap()), "should block {ip}");
        }
    }

    #[test]
    fn ssrf_denylist_allows_public_addresses() {
        for ip in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "2606:2800:220:1::1"] {
            assert!(!is_blocked_ip(&ip.parse().unwrap()), "should allow {ip}");
        }
    }
}
