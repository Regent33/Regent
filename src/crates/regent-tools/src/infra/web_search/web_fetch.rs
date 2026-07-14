//! `web_fetch` — bounded page fetch. Split from `web_search.rs`
//! (file-size rule).

use super::*;

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

/// Fetch a URL's text under the shared SSRF guard (`infra::net`): redirects
/// re-validated per hop, private IPs refused, body capped. Lossy-UTF8 decoded.
async fn guarded_fetch(url_str: &str) -> Result<(u16, String), String> {
    let (status, bytes) =
        net::guarded_get_bytes(url_str, FETCH_MAX_BYTES, HTTP_TIMEOUT_SECS).await?;
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

/// Strip `<script>`/`<style>` blocks and all tags, decode a few common
/// entities, and collapse whitespace. Good enough for feeding text to the model.
pub(super) fn html_to_text(html: &str) -> String {
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
