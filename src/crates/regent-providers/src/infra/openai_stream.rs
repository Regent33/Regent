//! SSE streaming for the OpenAI-compatible chat endpoint. Same wire shapes as
//! `adapters.rs`, delivered incrementally: `delta.content` fragments reach the
//! sink as they arrive, tool-call fragments accumulate by index, and the final
//! `ChatResponse` matches what `parse_response` would produce non-streaming.

mod accumulator;

use crate::domain::contracts::DeltaSink;
use crate::domain::entities::{ChatRequest, ChatResponse};
use crate::domain::errors::ProviderError;
use crate::infra::adapters::build_payload;
use crate::infra::http::{network_error, truncate};
use crate::infra::openai_compat::OpenAiCompatChatConfig;
use accumulator::StreamAccumulator;
use futures::StreamExt;
use serde_json::Value;

/// One streaming attempt: open the SSE stream, feed each `data:` line to the
/// accumulator, forward content fragments to `on_delta`. No retry — a partial
/// stream can't be replayed without double-emitting deltas.
pub(super) async fn stream_once(
    client: &reqwest::Client,
    config: &OpenAiCompatChatConfig,
    request: &ChatRequest,
    on_delta: DeltaSink<'_>,
) -> Result<ChatResponse, ProviderError> {
    let url = format!(
        "{}{}",
        config.base_url.trim_end_matches('/'),
        config.api_path
    );
    let mut payload = build_payload(&config.model, request);
    payload["stream"] = Value::Bool(true);
    payload["stream_options"] = serde_json::json!({"include_usage": true});
    let response = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .timeout(config.timeout)
        .json(&payload)
        .send()
        .await
        .map_err(|e| network_error(&e))?;
    let status = response.status().as_u16();
    if !(200..=299).contains(&status) {
        let retry_after_ms = crate::infra::http::retry_after_ms(response.headers());
        let body = response.text().await.unwrap_or_default();
        return Err(match status {
            401 | 403 => ProviderError::Auth { status },
            429 => ProviderError::RateLimited { retry_after_ms },
            // Redact before surfacing — an error body can echo our key.
            _ => ProviderError::Api {
                status,
                body: truncate(&regent_kernel::redact_secrets(&body), 600),
            },
        });
    }

    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    let mut acc = StreamAccumulator::default();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| network_error(&e))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let Some(data) = line.trim_end().strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<Value>(data)
                && let Some(fragment) = acc.push(&event)
            {
                on_delta(&fragment);
            }
        }
    }
    Ok(acc.finish())
}

#[cfg(test)]
#[path = "openai_stream_tests.rs"]
mod tests;
