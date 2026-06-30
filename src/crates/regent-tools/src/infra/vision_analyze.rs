//! `vision_analyze` — load an image and get a text description/answer (Hermes
//! `vision_tools.py` port, text path). Resolves an image from a local file
//! (path-jailed), a `data:` URL, or an `https` URL (SSRF-guarded + size-capped
//! via `infra::net`), base64-encodes it, and sends it to a vision-capable model
//! over an OpenAI-compatible endpoint, returning the analysis.
//!
//! Regent's chat contract is text-only, so this tool owns its own vision call
//! and returns TEXT — the agent reads the description on its next turn. The
//! vision model is configured by env (mirrors `web_search`'s provider/key
//! resolution): `REGENT_VISION_BASE_URL` · `REGENT_VISION_MODEL` ·
//! `REGENT_VISION_API_KEY` (falls back to `REGENT_API_KEY`).

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use crate::infra::net;
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::time::Duration;

const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024; // 20 MB — no provider accepts more
const DOWNLOAD_TIMEOUT_SECS: u64 = 30;
const VISION_TIMEOUT_SECS: u64 = 120;
const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_MODEL: &str = "google/gemini-2.5-flash";

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "vision_analyze".into(),
        description: "Analyze an image and return a text description/answer. Accepts an http(s) \
                      URL, a local file path, or a data: URL. Use whenever the user references an \
                      image (a path in their message, a URL, a screenshot). Needs a vision model \
                      configured via REGENT_VISION_MODEL + REGENT_VISION_API_KEY (falls back to \
                      REGENT_API_KEY)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "image_url": {"type": "string", "description": "http(s) URL, local file path, or data: URL of the image."},
                "question": {"type": "string", "description": "What to analyze or ask about the image."}
            },
            "required": ["image_url", "question"]
        }),
        toolset: "vision".into(),
    }
}

pub struct VisionAnalyzeTool;

#[async_trait]
impl ToolExecutor for VisionAnalyzeTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(image) = args.get("image_url").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: image_url"));
        };
        let question = args
            .get("question")
            .and_then(Value::as_str)
            .filter(|q| !q.trim().is_empty())
            .unwrap_or("Describe this image in detail.");

        let (mime, bytes) = match resolve_image(image, ctx).await {
            Ok(pair) => pair,
            Err(error) => return Ok(tool_error_json(error)),
        };
        if bytes.len() > MAX_IMAGE_BYTES {
            return Ok(tool_error_json(format!(
                "image too large ({} bytes, max {MAX_IMAGE_BYTES})",
                bytes.len()
            )));
        }
        let data_url = format!("data:{mime};base64,{}", B64.encode(&bytes));

        let base = env_or("REGENT_VISION_BASE_URL", DEFAULT_BASE_URL);
        let model = env_or("REGENT_VISION_MODEL", DEFAULT_MODEL);
        let Some(key) =
            nonempty_env("REGENT_VISION_API_KEY").or_else(|| nonempty_env("REGENT_API_KEY"))
        else {
            return Ok(tool_error_json(
                "vision_analyze needs an API key — set REGENT_VISION_API_KEY (or REGENT_API_KEY)",
            ));
        };

        match call_vision(&base, &model, &key, question, &data_url).await {
            Ok(analysis) => Ok(json!({"success": true, "analysis": analysis}).to_string()),
            Err(error) => Ok(tool_error_json(format!("vision_analyze failed: {error}"))),
        }
    }
}

fn env_or(var: &str, default: &str) -> String {
    nonempty_env(var).unwrap_or_else(|| default.to_owned())
}

fn nonempty_env(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|s| !s.trim().is_empty())
}

/// Resolve the image source to `(mime, bytes)`. Three sources: a `data:` URL
/// (base64), an `http(s)` URL (SSRF-guarded download), or a local file (jailed).
/// The mime is sniffed from magic bytes (never trusted from the URL/header).
async fn resolve_image(image: &str, ctx: &ToolContext) -> Result<(String, Vec<u8>), String> {
    if let Some(rest) = image.strip_prefix("data:") {
        let (_meta, b64) = rest.split_once(',').ok_or("malformed data: URL")?;
        let bytes = B64
            .decode(b64.trim())
            .map_err(|e| format!("bad base64 in data: URL: {e}"))?;
        let mime = sniff_mime(&bytes).ok_or("data: URL is not a recognized image")?;
        return Ok((mime, bytes));
    }
    if image.starts_with("http://") || image.starts_with("https://") {
        let (_status, bytes) =
            net::guarded_get_bytes(image, MAX_IMAGE_BYTES, DOWNLOAD_TIMEOUT_SECS).await?;
        let mime = sniff_mime(&bytes)
            .ok_or("downloaded data is not a recognized image (png/jpeg/gif/webp/bmp)")?;
        return Ok((mime, bytes));
    }
    let resolved = ctx.resolve(image).map_err(|e| e.to_string())?;
    let bytes = tokio::fs::read(&resolved)
        .await
        .map_err(|e| format!("cannot read {}: {e}", resolved.display()))?;
    let mime =
        sniff_mime(&bytes).ok_or("file is not a recognized image (png/jpeg/gif/webp/bmp)")?;
    Ok((mime, bytes))
}

/// Magic-byte image mime sniff (ported from Hermes `_detect_image_mime_type`).
fn sniff_mime(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png".to_owned());
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg".to_owned());
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif".to_owned());
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp".to_owned());
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp".to_owned());
    }
    None
}

/// One OpenAI-compatible vision completion: a user message with a text part and
/// an `image_url` part (the base64 data URL). Returns the assistant text.
async fn call_vision(
    base: &str,
    model: &str,
    key: &str,
    question: &str,
    data_url: &str,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let body = json!({
        "model": model,
        "max_tokens": 2000,
        "temperature": 0.1,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": question},
                {"type": "image_url", "image_url": {"url": data_url}}
            ]
        }]
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(VISION_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("bad response: {e}"))?;
    if !status.is_success() {
        let msg = v["error"]["message"].as_str().unwrap_or("unknown error");
        return Err(format!("HTTP {}: {msg}", status.as_u16()));
    }
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_owned();
    if content.is_empty() {
        return Err("model returned empty content".into());
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;
    use std::sync::Arc;

    const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00";

    #[test]
    fn sniffs_known_image_magic_bytes() {
        assert_eq!(sniff_mime(PNG_SIG).as_deref(), Some("image/png"));
        assert_eq!(
            sniff_mime(&[0xff, 0xd8, 0xff, 0x00]).as_deref(),
            Some("image/jpeg")
        );
        assert_eq!(sniff_mime(b"GIF89a...").as_deref(), Some("image/gif"));
        assert_eq!(
            sniff_mime(b"RIFF1234WEBPxxxx").as_deref(),
            Some("image/webp")
        );
        assert_eq!(sniff_mime(b"not an image"), None);
    }

    #[tokio::test]
    async fn resolves_a_data_url() {
        let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll));
        let data_url = format!("data:image/png;base64,{}", B64.encode(PNG_SIG));
        let (mime, bytes) = resolve_image(&data_url, &ctx).await.unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, PNG_SIG);
    }

    #[tokio::test]
    async fn resolves_a_local_image_file() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.png"), PNG_SIG)
            .await
            .unwrap();
        let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
        let (mime, _bytes) = resolve_image("a.png", &ctx).await.unwrap();
        assert_eq!(mime, "image/png");
    }

    #[tokio::test]
    async fn non_image_file_is_rejected_before_any_network_call() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.txt"), "hello")
            .await
            .unwrap();
        let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
        let out = VisionAnalyzeTool
            .execute(json!({"image_url": "a.txt", "question": "?"}), &ctx)
            .await
            .unwrap();
        assert!(out.contains("not a recognized image"), "got: {out}");
    }
}
