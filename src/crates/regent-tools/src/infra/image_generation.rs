//! `image_generation` — generate an image from a text prompt (Hermes
//! `image_generation_tool` gap). Self-contained: POSTs to an OpenAI-compatible
//! `/images/generations` endpoint (`response_format=b64_json`), saves the PNG
//! to the artifacts area, reveals it, and returns the path. Env-configured like
//! `vision_analyze`: `REGENT_IMAGE_BASE_URL` · `REGENT_IMAGE_MODEL` ·
//! `REGENT_IMAGE_API_KEY` (falls back to `REGENT_API_KEY`).

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-image-1";
const TIMEOUT_SECS: u64 = 180;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "image_generation".into(),
        description: "Generate an image from a text prompt and save it to disk; returns the file \
                      path (reveal it / send it to the user). Optional `size` (e.g. 1024x1024). \
                      Needs an image model configured via REGENT_IMAGE_MODEL + REGENT_IMAGE_API_KEY \
                      (falls back to REGENT_API_KEY)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "prompt": {"type": "string", "description": "What to generate."},
                "size": {"type": "string", "description": "Image size, e.g. 1024x1024 (optional)."}
            },
            "required": ["prompt"]
        }),
        toolset: "media".into(),
    }
}

pub struct ImageGenerationTool;

#[async_trait]
impl ToolExecutor for ImageGenerationTool {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(prompt) = args
            .get("prompt")
            .and_then(Value::as_str)
            .filter(|p| !p.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: prompt"));
        };
        let size = args.get("size").and_then(Value::as_str);
        let base = nonempty_env("REGENT_IMAGE_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.into());
        let model = nonempty_env("REGENT_IMAGE_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into());
        let Some(key) =
            nonempty_env("REGENT_IMAGE_API_KEY").or_else(|| nonempty_env("REGENT_API_KEY"))
        else {
            return Ok(tool_error_json(
                "image_generation needs an API key — set REGENT_IMAGE_API_KEY (or REGENT_API_KEY)",
            ));
        };

        let bytes = match generate(&base, &model, &key, prompt, size).await {
            Ok(bytes) => bytes,
            Err(error) => return Ok(tool_error_json(format!("image_generation failed: {error}"))),
        };
        // Save under the cwd's artifacts area (one file per object); reveal it.
        let dir = ctx.cwd.join("artifacts");
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            return Ok(tool_error_json(format!("cannot create artifacts dir: {e}")));
        }
        let path = dir.join(format!("image-{}.png", uuid::Uuid::new_v4().simple()));
        if let Err(e) = tokio::fs::write(&path, &bytes).await {
            return Ok(tool_error_json(format!("cannot save image: {e}")));
        }
        crate::infra::reveal::reveal(&path);
        Ok(
            json!({"ok": true, "path": path.display().to_string(), "bytes": bytes.len()})
                .to_string(),
        )
    }
}

fn nonempty_env(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|s| !s.trim().is_empty())
}

/// POST an OpenAI-compatible image generation request and return the PNG bytes.
async fn generate(
    base: &str,
    model: &str,
    key: &str,
    prompt: &str,
    size: Option<&str>,
) -> Result<Vec<u8>, String> {
    let url = format!("{}/images/generations", base.trim_end_matches('/'));
    let mut body = json!({"model": model, "prompt": prompt, "n": 1, "response_format": "b64_json"});
    if let Some(size) = size {
        body["size"] = json!(size);
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
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
    let b64 = v["data"][0]["b64_json"]
        .as_str()
        .ok_or("response had no data[0].b64_json")?;
    B64.decode(b64.trim())
        .map_err(|e| format!("bad base64: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;
    use std::sync::Arc;

    #[tokio::test]
    async fn missing_prompt_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
        let out = ImageGenerationTool.execute(json!({}), &ctx).await.unwrap();
        assert!(out.contains("missing required parameter"), "got: {out}");
    }
}
