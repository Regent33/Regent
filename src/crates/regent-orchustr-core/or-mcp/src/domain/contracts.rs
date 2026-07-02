#![allow(async_fn_in_trait)]

use crate::domain::entities::{JsonRpcMessage, McpTool};
use crate::domain::errors::McpError;

#[cfg_attr(test, mockall::automock)]
pub trait NexusClientTrait: Send + Sync + 'static {
    async fn send(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError>;
    async fn list_tools(&self) -> Result<Vec<McpTool>, McpError>;
    async fn invoke_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError>;
}

#[cfg_attr(test, mockall::automock)]
pub trait NexusServerTrait: Send + Sync + 'static {
    fn register_tool(&mut self, tool: McpTool) -> Result<(), McpError>;
    async fn serve(&self) -> Result<(), McpError>;
}

#[cfg_attr(test, mockall::automock)]
pub trait McpTransport: Send + Sync + 'static {
    async fn send_message(&self, msg: &JsonRpcMessage) -> Result<(), McpError>;
    async fn receive_message(&self) -> Result<JsonRpcMessage, McpError>;
}
