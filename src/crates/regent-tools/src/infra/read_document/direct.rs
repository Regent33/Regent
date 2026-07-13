//! First rung of the document ladder: hand the raw PDF to the model itself
//! (OpenAI-compatible `file` content part, the OpenRouter PDF shape) and let
//! it read pictures, layout, and links no local extractor sees. Any failure
//! degrades to local extraction with the named reason riding in the result.
//! Uses the same env resolution as `vision_analyze`: `REGENT_VISION_BASE_URL`
//! · `REGENT_VISION_MODEL` · `REGENT_VISION_API_KEY` (→ `REGENT_API_KEY`).
// ponytail: PDFs only — no provider takes docx/pptx/xlsx natively; those go
// straight to local extraction + vision on the embedded images.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

const MAX_DOC_BYTES: usize = 20 * 1024 * 1024;
const TIMEOUT_SECS: u64 = 180;
const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_MODEL: &str = "google/gemini-2.5-flash";

const READ_PROMPT: &str = "Read this document completely. Return: (1) the full text content \
     (headings + body, tables as rows); (2) a one-line description of every figure/image/chart; \
     (3) any hyperlinks you can see. Be faithful — transcribe, don't summarize.";

/// Sends the PDF to the configured vision/document model. `Err` carries a
/// CLEAR per-provider reason (which endpoint, what it said) — callers fall
/// back to local extraction and surface that reason in the result, so a
/// provider that can't take file inputs is never a silent mystery.
pub(super) async fn model_reads_pdf(path: &Path) -> Result<String, String> {
    let Some(key) =
        nonempty_env("REGENT_VISION_API_KEY").or_else(|| nonempty_env("REGENT_API_KEY"))
    else {
        return Err(
            "no API key for a direct model read (set REGENT_VISION_API_KEY or REGENT_API_KEY)"
                .into(),
        );
    };
    let bytes = match tokio::fs::read(path).await {
        Ok(b) if b.len() <= MAX_DOC_BYTES => b,
        Ok(b) => {
            return Err(format!(
                "document is {} bytes (direct-read cap {MAX_DOC_BYTES})",
                b.len()
            ));
        }
        Err(error) => return Err(format!("cannot read the file: {error}")),
    };
    let base = nonempty_env("REGENT_VISION_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.into());
    let model = nonempty_env("REGENT_VISION_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into());
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document.pdf");
    let body = json!({
        "model": model,
        "max_tokens": 8000,
        "temperature": 0,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": READ_PROMPT},
                {"type": "file", "file": {
                    "filename": filename,
                    "file_data": format!("data:application/pdf;base64,{}", B64.encode(&bytes)),
                }}
            ]
        }]
    });
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .post(&url)
        .bearer_auth(&key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request to {base} failed: {e}"))?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| {
        format!(
            "{base} returned HTTP {} with a non-JSON body: {e}",
            status.as_u16()
        )
    })?;
    if !status.is_success() {
        let detail = v["error"]["message"].as_str().unwrap_or("no detail");
        // 400/404/422 on a file part almost always means the provider/model
        // doesn't take document inputs — name that plainly.
        let hint = if matches!(status.as_u16(), 400 | 404 | 415 | 422) {
            format!(" — '{model}' via {base} likely doesn't accept file/document inputs")
        } else {
            String::new()
        };
        return Err(format!(
            "HTTP {} from {base}: {detail}{hint}",
            status.as_u16()
        ));
    }
    let content = message_text(&v["choices"][0]["message"]["content"]);
    if content.is_empty() {
        return Err(format!(
            "'{model}' returned an empty reply for the document"
        ));
    }
    Ok(content)
}

/// `content` as text: a plain string, or the OpenAI array-of-parts shape
/// (`[{"type":"text","text":...}, …]`) some providers reply with.
fn message_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.trim().to_owned(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned(),
        _ => String::new(),
    }
}

fn nonempty_env(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|s| !s.trim().is_empty())
}
