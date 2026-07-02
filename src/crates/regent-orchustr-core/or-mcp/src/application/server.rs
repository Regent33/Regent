use crate::application::server_handlers::{call_tool, get_task};
use crate::domain::contracts::{McpTransport, NexusServerTrait};
use crate::domain::entities::{
    JsonRpcErrorDetail, JsonRpcErrorResponse, JsonRpcMessage, JsonRpcRequest,
    JsonRpcSuccessResponse, McpTask, McpTool, ServerCard,
};
use crate::domain::errors::McpError;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

type ToolFuture =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, McpError>> + Send + 'static>>;
type ToolHandler = Arc<dyn Fn(serde_json::Value) -> ToolFuture + Send + Sync + 'static>;

#[derive(Clone)]
pub(crate) struct RegisteredTool {
    pub(crate) tool: McpTool,
    pub(crate) handler: Option<ToolHandler>,
}

pub struct NexusServer<T: McpTransport> {
    transport: T,
    tools: Arc<Mutex<HashMap<String, RegisteredTool>>>,
    tasks: Arc<Mutex<HashMap<String, McpTask>>>,
    server_card: ServerCard,
}

impl<T: McpTransport> NexusServer<T> {
    #[must_use]
    pub fn new(transport: T, server_card: ServerCard) -> Self {
        Self {
            transport,
            tools: Arc::new(Mutex::new(HashMap::new())),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            server_card,
        }
    }

    pub async fn register_tool_handler<F, Fut>(
        &mut self,
        tool: McpTool,
        handler: F,
    ) -> Result<(), McpError>
    where
        F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<serde_json::Value, McpError>> + Send + 'static,
    {
        let mut tools = self.tools.lock().await;
        if tools.contains_key(&tool.name) {
            return Err(McpError::ToolExecution(format!(
                "duplicate tool: {}",
                tool.name
            )));
        }
        tools.insert(
            tool.name.clone(),
            RegisteredTool {
                tool,
                handler: Some(Arc::new(move |args| Box::pin(handler(args)))),
            },
        );
        Ok(())
    }

    pub async fn register_task(&self, task: McpTask) {
        self.tasks.lock().await.insert(task.id.clone(), task);
    }

    #[must_use]
    pub fn server_card(&self) -> &ServerCard {
        &self.server_card
    }

    #[must_use]
    pub fn server_card_path(&self) -> &'static str {
        "/.well-known/mcp-server"
    }

    pub fn server_card_json(&self) -> Result<String, McpError> {
        serde_json::to_string(&self.server_card)
            .map_err(|error| McpError::Serialization(error.to_string()))
    }

    pub async fn handle_message(
        &self,
        message: JsonRpcMessage,
    ) -> Result<Option<JsonRpcMessage>, McpError> {
        match message {
            JsonRpcMessage::Request(request) => self.handle_request(request).await.map(Some),
            JsonRpcMessage::Notification(_) => Ok(None),
            JsonRpcMessage::Success(_) | JsonRpcMessage::Error(_) => Ok(None),
        }
    }

    async fn handle_request(&self, request: JsonRpcRequest) -> Result<JsonRpcMessage, McpError> {
        let result = match request.method.as_str() {
            "initialize" => Ok(serde_json::json!({
                "protocolVersion": self.server_card.protocol_version,
                "serverInfo": { "name": self.server_card.name, "version": self.server_card.version },
                "capabilities": { "tools": {}, "prompts": {}, "resources": {} }
            })),
            "ping" => Ok(serde_json::json!({ "pong": true })),
            "tools/list" => {
                let tools = self
                    .tools
                    .lock()
                    .await
                    .values()
                    .map(|entry| entry.tool.clone())
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({ "tools": tools }))
            }
            "tools/call" => call_tool(&self.tools, request.params.clone()).await,
            "tasks/get" => get_task(&self.tasks, request.params.clone()).await,
            "shutdown" => Ok(serde_json::json!({ "shutdown": true })),
            other => Err(McpError::Protocol(format!("unsupported method: {other}"))),
        };

        match result {
            Ok(result) => Ok(JsonRpcMessage::Success(JsonRpcSuccessResponse {
                jsonrpc: "2.0".to_owned(),
                id: request.id,
                result,
            })),
            Err(error) => Ok(JsonRpcMessage::Error(JsonRpcErrorResponse {
                jsonrpc: "2.0".to_owned(),
                id: Some(request.id),
                error: JsonRpcErrorDetail {
                    code: -32602,
                    message: error.to_string(),
                },
            })),
        }
    }
}

impl<T: McpTransport> NexusServerTrait for NexusServer<T> {
    fn register_tool(&mut self, tool: McpTool) -> Result<(), McpError> {
        if let Ok(mut tools) = self.tools.try_lock() {
            if tools.contains_key(&tool.name) {
                return Err(McpError::ToolExecution(format!(
                    "duplicate tool: {}",
                    tool.name
                )));
            }
            tools.insert(
                tool.name.clone(),
                RegisteredTool {
                    tool,
                    handler: None,
                },
            );
            Ok(())
        } else {
            Err(McpError::ToolExecution(
                "tool registry is currently busy".to_owned(),
            ))
        }
    }

    async fn serve(&self) -> Result<(), McpError> {
        loop {
            let message = self.transport.receive_message().await?;
            if let Some(response) = self.handle_message(message).await? {
                let is_shutdown = matches!(
                    &response,
                    JsonRpcMessage::Success(JsonRpcSuccessResponse { result, .. })
                        if result["shutdown"].as_bool() == Some(true)
                );
                self.transport.send_message(&response).await?;
                if is_shutdown {
                    return Ok(());
                }
            }
        }
    }
}
