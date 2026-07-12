//! Any OpenAI-compatible chat-completions endpoint with native tool calling.
//! One adapter serves every such provider — the base URL is the only thing
//! that changes — so the named presets below (OpenAI, OpenRouter, Groq,
//! DeepSeek, Together, Ollama) are just `new` with the right URL.

use crate::domain::contracts::{ChatProvider, DeltaSink};
use crate::domain::entities::{ChatRequest, ChatResponse};
use crate::domain::errors::ProviderError;
use crate::infra::adapters::{build_payload, parse_response};
use crate::infra::http::{run_with_retry, truncate};
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
            timeout: Duration::from_secs(120),
        }
    }

    /// OpenAI (`api.openai.com`).
    #[must_use]
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.openai.com", api_key, model)
    }

    /// OpenRouter (`openrouter.ai`) — hundreds of models behind one key.
    #[must_use]
    pub fn openrouter(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://openrouter.ai/api", api_key, model)
    }

    /// Groq (`api.groq.com`) — fast hosted open models.
    #[must_use]
    pub fn groq(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.groq.com/openai", api_key, model)
    }

    /// DeepSeek (`api.deepseek.com`).
    #[must_use]
    pub fn deepseek(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.deepseek.com", api_key, model)
    }

    /// Together AI (`api.together.xyz`).
    #[must_use]
    pub fn together(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.together.xyz", api_key, model)
    }

    /// Local Ollama (`localhost:11434`, no key) via its OpenAI-compat endpoint.
    #[must_use]
    pub fn ollama(model: impl Into<String>) -> Self {
        Self::new("http://localhost:11434", "", model)
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

impl OpenAiCompatChat {
    #[must_use]
    pub fn new(config: OpenAiCompatChatConfig) -> Self {
        Self {
            config,
            // Bound the two ways a dead provider stalls a turn before failover:
            // a connect timeout for one that won't accept the connection (a down
            // endpoint — no healthy host needs >10s to connect), and a read
            // timeout for one that connects but never sends a byte (a hung
            // endpoint). Both turn the stall into a fast, failover-able error,
            // well clear of any healthy stream's sub-second inter-token gaps.
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(60))
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
                parse_response(&body)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_target_the_expected_base_urls() {
        assert_eq!(
            OpenAiCompatChatConfig::openai("k", "m").base_url,
            "https://api.openai.com"
        );
        assert_eq!(
            OpenAiCompatChatConfig::openrouter("k", "m").base_url,
            "https://openrouter.ai/api"
        );
        assert_eq!(
            OpenAiCompatChatConfig::groq("k", "m").base_url,
            "https://api.groq.com/openai"
        );
        assert_eq!(
            OpenAiCompatChatConfig::deepseek("k", "m").base_url,
            "https://api.deepseek.com"
        );
        assert_eq!(
            OpenAiCompatChatConfig::together("k", "m").base_url,
            "https://api.together.xyz"
        );
    }

    #[test]
    fn ollama_is_local_and_keyless() {
        let cfg = OpenAiCompatChatConfig::ollama("llama3");
        assert_eq!(cfg.base_url, "http://localhost:11434");
        assert_eq!(cfg.api_key, "");
        assert_eq!(cfg.api_path, "/v1/chat/completions");
    }
}
