use crate::application::server::RegisteredTool;
use crate::application::server_validation::validate_input;
use crate::domain::entities::McpTask;
use crate::domain::errors::McpError;
use std::collections::HashMap;

pub(crate) async fn call_tool(
    tools: &tokio::sync::Mutex<HashMap<String, RegisteredTool>>,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, McpError> {
    let params =
        params.ok_or_else(|| McpError::Protocol("tools/call requires params".to_owned()))?;
    let name = params["name"]
        .as_str()
        .ok_or_else(|| McpError::Protocol("tools/call requires a string name".to_owned()))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let tools = tools.lock().await;
    let tool = tools
        .get(name)
        .ok_or_else(|| McpError::ToolExecution(format!("unknown tool: {name}")))?;
    validate_input(&tool.tool.input_schema, &args)?;
    let handler = tool
        .handler
        .clone()
        .ok_or_else(|| McpError::ToolExecution(format!("tool handler not configured: {name}")))?;
    handler(args).await
}

pub(crate) async fn get_task(
    tasks: &tokio::sync::Mutex<HashMap<String, McpTask>>,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, McpError> {
    let params =
        params.ok_or_else(|| McpError::Protocol("tasks/get requires params".to_owned()))?;
    let id = params["id"]
        .as_str()
        .ok_or_else(|| McpError::Protocol("tasks/get requires a string id".to_owned()))?;
    let task = tasks
        .lock()
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| McpError::TaskExpired(id.to_owned()))?;
    serde_json::to_value(task).map_err(|error| McpError::Serialization(error.to_string()))
}
