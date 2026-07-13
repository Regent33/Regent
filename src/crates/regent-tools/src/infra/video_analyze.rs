//! `video_analyze` — analyze a video and return a text description/answer
//! (Hermes `video_analyze` gap). Mirrors `vision_analyze`: resolves a local
//! file (jailed), a `data:` URL, or an `https` URL (SSRF-guarded + 50 MB cap),
//! base64-encodes it, and sends a `video_url` content part to a video-capable
//! model over an OpenAI-compatible endpoint, returning the analysis text.
//!
//! Reuses the vision provider config (same multimodal endpoint), with an
//! optional `REGENT_VIDEO_MODEL` override: `REGENT_VISION_BASE_URL` ·
//! `REGENT_VIDEO_MODEL`→`REGENT_VISION_MODEL` · `REGENT_VISION_API_KEY`
//! (falls back to `REGENT_API_KEY`).

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use crate::infra::net;
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::time::Duration;

const MAX_VIDEO_BYTES: usize = 50 * 1024 * 1024; // 50 MB hard cap
const DOWNLOAD_TIMEOUT_SECS: u64 = 60;
const ANALYZE_TIMEOUT_SECS: u64 = 180;
const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_MODEL: &str = "google/gemini-2.5-flash";

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "video_analyze".into(),
        description: "Analyze a video and return a text description/answer. Accepts an http(s) \
                      URL, a local file path, or a data: URL (mp4/webm/mov, ≤50 MB). For images \
                      use vision_analyze. Needs a video-capable model via REGENT_VIDEO_MODEL \
                      (or REGENT_VISION_MODEL) + REGENT_VISION_API_KEY (falls back to REGENT_API_KEY)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "video_url": {"type": "string", "description": "http(s) URL, local file path, or data: URL of the video."},
                "question": {"type": "string", "description": "What to analyze or ask about the video."}
            },
            "required": ["video_url", "question"]
        }),
        toolset: "media".into(),
    }
}

pub struct VideoAnalyzeTool;

#[async_trait]
impl ToolExecutor for VideoAnalyzeTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(video) = args.get("video_url").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: video_url"));
        };
        let question = args
            .get("question")
            .and_then(Value::as_str)
            .filter(|q| !q.trim().is_empty())
            .unwrap_or("Describe what happens in this video.");

        let (mime, bytes) = match resolve_video(video, ctx).await {
            Ok(pair) => pair,
            Err(error) => return Ok(tool_error_json(error)),
        };
        if bytes.len() > MAX_VIDEO_BYTES {
            return Ok(tool_error_json(format!(
                "video too large ({} bytes, max {MAX_VIDEO_BYTES})",
                bytes.len()
            )));
        }
        let data_url = format!("data:{mime};base64,{}", B64.encode(&bytes));

        let base =
            nonempty_env("REGENT_VISION_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.into());
        let model = nonempty_env("REGENT_VIDEO_MODEL")
            .or_else(|| nonempty_env("REGENT_VISION_MODEL"))
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        let Some(key) =
            nonempty_env("REGENT_VISION_API_KEY").or_else(|| nonempty_env("REGENT_API_KEY"))
        else {
            return Ok(tool_error_json(
                "video_analyze needs an API key — set REGENT_VISION_API_KEY (or REGENT_API_KEY)",
            ));
        };

        match call_video(&base, &model, &key, question, &data_url).await {
            Ok(analysis) => Ok(json!({"success": true, "analysis": analysis}).to_string()),
            Err(error) => Ok(tool_error_json(format!("video_analyze failed: {error}"))),
        }
    }
}

fn nonempty_env(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|s| !s.trim().is_empty())
}

/// Resolve the video to `(mime, bytes)`: `data:` URL, `http(s)` (SSRF-guarded),
/// or a jailed local file. Mime is by extension (videos lack a simple magic).
async fn resolve_video(video: &str, ctx: &ToolContext) -> Result<(String, Vec<u8>), String> {
    if let Some(rest) = video.strip_prefix("data:") {
        let (meta, b64) = rest.split_once(',').ok_or("malformed data: URL")?;
        let mime = meta
            .split(';')
            .next()
            .filter(|m| !m.is_empty())
            .unwrap_or("video/mp4");
        let bytes = B64
            .decode(b64.trim())
            .map_err(|e| format!("bad base64 in data: URL: {e}"))?;
        return Ok((mime.to_owned(), bytes));
    }
    if video.starts_with("http://") || video.starts_with("https://") {
        let (_status, bytes) =
            net::guarded_get_bytes(video, MAX_VIDEO_BYTES, DOWNLOAD_TIMEOUT_SECS).await?;
        return Ok((mime_for(video), bytes));
    }
    let resolved = ctx.resolve(video).map_err(|e| e.to_string())?;
    let bytes = tokio::fs::read(&resolved)
        .await
        .map_err(|e| format!("cannot read {}: {e}", resolved.display()))?;
    Ok((mime_for(&resolved.to_string_lossy()), bytes))
}

/// Extension → video mime (avi/mkv fall back to mp4, matching Hermes).
fn mime_for(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "webm" => "video/webm",
        "mov" => "video/mov",
        "mpeg" | "mpg" => "video/mpeg",
        _ => "video/mp4",
    }
    .to_owned()
}

async fn call_video(
    base: &str,
    model: &str,
    key: &str,
    question: &str,
    data_url: &str,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let body = json!({
        "model": model,
        "max_tokens": 4000,
        "temperature": 0.1,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": question},
                {"type": "video_url", "video_url": {"url": data_url}}
            ]
        }]
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(ANALYZE_TIMEOUT_SECS))
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
#[path = "video_analyze_tests.rs"]
mod tests;
