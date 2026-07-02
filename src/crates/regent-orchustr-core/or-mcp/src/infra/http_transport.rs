use crate::domain::contracts::McpTransport;
use crate::domain::entities::JsonRpcMessage;
use crate::domain::errors::McpError;
use crate::infra::jsonrpc::{decode_streamable_body, encode_message};
use reqwest::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct StreamableHttpTransport {
    client: Client,
    endpoint: String,
    auth_token: Option<String>,
    inbox: Arc<Mutex<VecDeque<JsonRpcMessage>>>,
    session_id: Arc<Mutex<Option<String>>>,
}

impl StreamableHttpTransport {
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self::with_bearer_token(endpoint, None::<String>)
    }

    /// Creates an MCP streamable HTTP transport with an optional bearer token.
    #[must_use]
    pub fn with_bearer_token(
        endpoint: impl Into<String>,
        auth_token: impl Into<Option<String>>,
    ) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.into(),
            auth_token: auth_token.into(),
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            session_id: Arc::new(Mutex::new(None)),
        }
    }
}

impl McpTransport for StreamableHttpTransport {
    async fn send_message(&self, msg: &JsonRpcMessage) -> Result<(), McpError> {
        let mut request = self
            .client
            .post(&self.endpoint)
            .header(ACCEPT, "application/json, text/event-stream");
        if let Some(token) = &self.auth_token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(session_id) = self.session_id.lock().await.clone() {
            request = request.header("Mcp-Session-Id", session_id);
        }
        let response = request.body(encode_message(msg)?).send().await?;
        if let Some(session_id) = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
        {
            *self.session_id.lock().await = Some(session_id.to_owned());
        }
        let body = response.text().await?;
        if let Some(message) = decode_streamable_body(&body)? {
            self.inbox.lock().await.push_back(message);
        }
        Ok(())
    }

    async fn receive_message(&self) -> Result<JsonRpcMessage, McpError> {
        if let Some(message) = self.inbox.lock().await.pop_front() {
            return Ok(message);
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        if let Some(token) = &self.auth_token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))
                    .map_err(|error| McpError::Protocol(error.to_string()))?,
            );
        }
        if let Some(session_id) = self.session_id.lock().await.clone() {
            headers.insert(
                "Mcp-Session-Id",
                HeaderValue::from_str(&session_id)
                    .map_err(|error| McpError::Protocol(error.to_string()))?,
            );
        }
        let response = self
            .client
            .get(&self.endpoint)
            .headers(headers)
            .send()
            .await?;
        let body = response.text().await?;
        decode_streamable_body(&body)?.ok_or_else(|| {
            McpError::Transport("streamable HTTP response contained no JSON-RPC message".to_owned())
        })
    }
}
