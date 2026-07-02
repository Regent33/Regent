use crate::domain::contracts::{McpTransport, NexusClientTrait};
use crate::domain::entities::{
    JsonRpcErrorResponse, JsonRpcId, JsonRpcMessage, JsonRpcRequest, JsonRpcSuccessResponse,
    McpTask, McpTool,
};
use crate::domain::errors::McpError;
use crate::infra::http_transport::StreamableHttpTransport;
use crate::infra::stdio_transport::StdioTransport;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

#[derive(Clone)]
pub struct NexusClient<T: McpTransport> {
    transport: T,
    next_id: Arc<AtomicI64>,
}

impl<T: McpTransport> NexusClient<T> {
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            next_id: Arc::new(AtomicI64::new(1)),
        }
    }

    pub async fn initialize(&self) -> Result<serde_json::Value, McpError> {
        self.send(
            "initialize",
            serde_json::json!({ "protocolVersion": "2025-11-25" }),
        )
        .await
    }

    pub async fn ping(&self) -> Result<serde_json::Value, McpError> {
        self.send("ping", serde_json::json!({})).await
    }

    pub async fn get_task(&self, id: &str) -> Result<McpTask, McpError> {
        let value = self
            .send("tasks/get", serde_json::json!({ "id": id }))
            .await?;
        serde_json::from_value(value).map_err(|error| McpError::Serialization(error.to_string()))
    }

    async fn send_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<JsonRpcSuccessResponse, McpError> {
        let id = JsonRpcId::Number(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.transport
            .send_message(&JsonRpcMessage::Request(JsonRpcRequest {
                jsonrpc: "2.0".to_owned(),
                id: id.clone(),
                method: method.to_owned(),
                params: Some(params),
            }))
            .await?;
        match self.transport.receive_message().await? {
            JsonRpcMessage::Success(response) if response.id == id => Ok(response),
            JsonRpcMessage::Error(JsonRpcErrorResponse { error, .. }) => {
                Err(McpError::Protocol(error.message))
            }
            _ => Err(McpError::Protocol(
                "unexpected JSON-RPC response message".to_owned(),
            )),
        }
    }
}

impl NexusClient<StreamableHttpTransport> {
    #[must_use]
    pub fn connect_http(endpoint: impl Into<String>) -> Self {
        Self::new(StreamableHttpTransport::new(endpoint))
    }
}

impl NexusClient<StdioTransport> {
    pub fn connect_stdio(command: &str, args: &[&str]) -> Result<Self, McpError> {
        Ok(Self::new(StdioTransport::spawn(command, args)?))
    }
}

impl<T: McpTransport> NexusClientTrait for NexusClient<T> {
    async fn send(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        Ok(self.send_request(method, params).await?.result)
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>, McpError> {
        let value = self.send("tools/list", serde_json::json!({})).await?;
        serde_json::from_value(value["tools"].clone())
            .map_err(|error| McpError::Serialization(error.to_string()))
    }

    async fn invoke_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        self.send(
            "tools/call",
            serde_json::json!({ "name": name, "arguments": args }),
        )
        .await
    }
}
