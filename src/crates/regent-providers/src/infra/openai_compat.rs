//! Any OpenAI-compatible chat-completions endpoint with native tool calling.
//! One adapter serves every such provider — the base URL is the only thing
//! that changes — so the named presets below (OpenAI, OpenRouter, Groq,
//! DeepSeek, Together, Ollama) are just `new` with the right URL.

mod presets;

use crate::domain::contracts::{ChatProvider, DeltaSink};
use crate::domain::entities::{ChatRequest, ChatResponse};
use crate::domain::errors::ProviderError;
use crate::infra::adapters::{build_payload, parse_response};
use crate::infra::http::{network_error, run_with_retry, truncate};
use crate::infra::window_discovery;
use async_trait::async_trait;
use or_core::RetryPolicy;
use reqwest::Client;
use std::fmt;
use std::time::Duration;

pub struct OpenAiCompatChat {
    config: OpenAiCompatChatConfig,
    client: Client,
    retry: RetryPolicy,
}

#[derive(Clone)]
pub struct OpenAiCompatChatConfig {
    pub base_url: String,
    pub api_path: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
}

impl OpenAiCompatChatConfig {
    #[must_use]
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_path: "/v1/chat/completions".to_owned(),
            api_key: api_key.into(),
            model: model.into(),
            timeout: crate::infra::http::REQUEST_TIMEOUT,
        }
    }
}

impl fmt::Debug for OpenAiCompatChat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiCompatChat")
            .field("base_url", &self.config.base_url)
            .field("model", &self.config.model)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// OpenRouter's catalog endpoint lives under `{base}/v1` — one string used by
/// both the fetch and the cache lookup so the key always matches.
fn openrouter_discovery_base(base_url: &str) -> String {
    format!("{}/v1", base_url.trim_end_matches('/'))
}

impl OpenAiCompatChat {
    #[must_use]
    pub fn new(config: OpenAiCompatChatConfig) -> Self {
        // Best-effort live window discovery — OpenRouter is the one
        // OpenAI-compatible host with a catalog that exposes context_length;
        // everything else stays on the static table / config override.
        if config.base_url.contains("openrouter") {
            window_discovery::spawn_openrouter_discovery(openrouter_discovery_base(
                &config.base_url,
            ));
        }
        Self {
            config,
            // A connect timeout bounds a down endpoint that won't accept the
            // connection (no healthy host needs >10s); a hung endpoint that
            // connects but never finishes is bounded by the total per-request
            // `.timeout(config.timeout)` on each send below. NO read timeout:
            // a large-prefill model can legitimately take >60s before its first
            // SSE byte, and a fixed read timeout mistook that slow first token
            // for a dead stream and failed over early (Nemotron regression).
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
            retry: RetryPolicy::default_llm(),
        }
    }

    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    async fn call_once(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!(
            "{}{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.api_path
        );
        let payload = build_payload(&self.config.model, request);
        let http_response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .timeout(self.config.timeout)
            .json(&payload)
            .send()
            .await
            .map_err(|e| network_error(&e))?;
        let status = http_response.status().as_u16();
        let retry_after_ms = crate::infra::http::retry_after_ms(http_response.headers());
        let body_text = http_response.text().await.map_err(|e| network_error(&e))?;
        match status {
            200..=299 => {
                let body: serde_json::Value = serde_json::from_str(&body_text)
                    .map_err(|e| ProviderError::Parse(e.to_string()))?;
                parse_response(&body)
            }
            401 | 403 => Err(ProviderError::Auth { status }),
            429 => Err(ProviderError::RateLimited { retry_after_ms }),
            // Redact before logging/surfacing — an error body can echo our key.
            _ => Err(ProviderError::Api {
                status,
                body: truncate(&regent_kernel::redact_secrets(&body_text), 600),
            }),
        }
    }
}

#[async_trait]
impl ChatProvider for OpenAiCompatChat {
    async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        run_with_retry(&self.retry, || self.call_once(request)).await
    }

    /// Single attempt — a partial SSE stream can't be replayed without
    /// double-emitting deltas (same policy as the Anthropic adapter).
    async fn complete_streaming(
        &self,
        request: &ChatRequest,
        on_delta: DeltaSink<'_>,
    ) -> Result<ChatResponse, ProviderError> {
        super::openai_stream::stream_once(&self.client, &self.config, request, on_delta).await
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    /// Discovered window (OpenRouter catalog, fetched in the background by
    /// `new`) first, static family table second. Non-OpenRouter hosts never
    /// populate the cache, so they read straight through to the table.
    fn context_window(&self) -> Option<u32> {
        window_discovery::discovered_window(
            &openrouter_discovery_base(&self.config.base_url),
            &self.config.model,
        )
        .or_else(|| crate::domain::model_windows::window_for_model(self.model()))
    }
}

#[cfg(test)]
#[path = "openai_compat_tests.rs"]
mod tests;
