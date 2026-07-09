//! Native Anthropic Messages API (`POST /v1/messages`) with `x-api-key` auth,
//! content-block tool calling, prompt-cache breakpoints, and SSE streaming.
//! Rust has no official Anthropic SDK, so this is a raw HTTP adapter (per the
//! claude-api guidance) sharing `or-core` retry/backoff. Wire translation lives
//! in the `anthropic` module.

use crate::domain::contracts::{ChatProvider, DeltaSink};
use crate::domain::entities::{ChatRequest, ChatResponse};
use crate::domain::errors::ProviderError;
use crate::infra::anthropic;
use crate::infra::http::{run_with_retry, truncate};
use async_trait::async_trait;
use futures::StreamExt;
use or_core::RetryPolicy;
use reqwest::Client;
use std::fmt;
use std::time::Duration;

pub struct AnthropicChat {
    config: AnthropicChatConfig,
    client: Client,
    retry: RetryPolicy,
}

#[derive(Clone)]
pub struct AnthropicChatConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub anthropic_version: String,
    pub timeout: Duration,
}

impl AnthropicChatConfig {
    #[must_use]
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let base = base_url.into();
        Self {
            base_url: if base.is_empty() {
                "https://api.anthropic.com".to_owned()
            } else {
                base
            },
            api_key: api_key.into(),
            model: model.into(),
            anthropic_version: "2023-06-01".to_owned(),
            timeout: Duration::from_secs(120),
        }
    }
}

impl fmt::Debug for AnthropicChat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicChat")
            .field("base_url", &self.config.base_url)
            .field("model", &self.config.model)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl AnthropicChat {
    #[must_use]
    pub fn new(config: AnthropicChatConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            retry: RetryPolicy::default_llm(),
        }
    }

    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    async fn call_once(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/v1/messages", self.config.base_url.trim_end_matches('/'));
        let payload = anthropic::build_payload(&self.config.model, request);
        let http_response = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", &self.config.anthropic_version)
            .header("content-type", "application/json")
            .timeout(self.config.timeout)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let status = http_response.status().as_u16();
        let body_text = http_response
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        match status {
            200..=299 => {
                let body: serde_json::Value = serde_json::from_str(&body_text)
                    .map_err(|e| ProviderError::Parse(e.to_string()))?;
                anthropic::parse_response(&body)
            }
            401 | 403 => Err(ProviderError::Auth { status }),
            429 => Err(ProviderError::RateLimited),
            // Redact before logging/surfacing — an error body can echo our key.
            _ => Err(ProviderError::Api {
                status,
                body: truncate(&regent_kernel::redact_secrets(&body_text), 600),
            }),
        }
    }

    /// One streaming attempt: open the SSE stream, feed each `data:` event to
    /// the accumulator, forward text fragments to `on_delta`. No retry — a
    /// partial stream can't be safely replayed.
    async fn stream_once(
        &self,
        request: &ChatRequest,
        on_delta: DeltaSink<'_>,
    ) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/v1/messages", self.config.base_url.trim_end_matches('/'));
        let payload = anthropic::build_streaming_payload(&self.config.model, request);
        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", &self.config.anthropic_version)
            .header("content-type", "application/json")
            .timeout(self.config.timeout)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return match status {
                401 | 403 => Err(ProviderError::Auth { status }),
                429 => Err(ProviderError::RateLimited),
                _ => Err(ProviderError::Api {
                    status,
                    body: truncate(&regent_kernel::redact_secrets(&body), 600),
                }),
            };
        }

        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        let mut acc = anthropic::StreamAccumulator::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ProviderError::Network(e.to_string()))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            // SSE frames are newline-delimited; only `data:` lines carry JSON.
            while let Some(nl) = buf.find('\n') {
                let line: String = buf.drain(..=nl).collect();
                let Some(data) = line.trim_end().strip_prefix("data: ") else {
                    continue;
                };
                if data == "[DONE]" {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data)
                    && let Some(delta) = acc.push(&event)
                {
                    on_delta(&delta);
                }
            }
        }
        Ok(acc.finish())
    }
}

#[async_trait]
impl ChatProvider for AnthropicChat {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        run_with_retry(&self.retry, || self.call_once(request)).await
    }

    async fn complete_streaming(
        &self,
        request: &ChatRequest,
        on_delta: DeltaSink<'_>,
    ) -> Result<ChatResponse, ProviderError> {
        // Single attempt — a partial SSE stream can't be replayed without
        // double-emitting deltas, so streaming opts out of the retry loop.
        self.stream_once(request, on_delta).await
    }

    fn model(&self) -> &str {
        &self.config.model
    }
}
