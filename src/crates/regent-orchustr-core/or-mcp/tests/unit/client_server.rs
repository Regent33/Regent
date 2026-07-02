use or_mcp::{
    JsonRpcId, JsonRpcMessage, JsonRpcRequest, JsonRpcSuccessResponse, McpTask, McpTool,
    McpTransport, NexusClient, NexusClientTrait, NexusServer, ServerCard,
};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct ScriptedTransport {
    sent: Arc<Mutex<Vec<JsonRpcMessage>>>,
    responses: Arc<Mutex<VecDeque<JsonRpcMessage>>>,
}

impl ScriptedTransport {
    fn with_responses(responses: Vec<JsonRpcMessage>) -> Self {
        Self {
            sent: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
        }
    }
}

impl McpTransport for ScriptedTransport {
    async fn send_message(&self, msg: &JsonRpcMessage) -> Result<(), or_mcp::McpError> {
        self.sent.lock().await.push(msg.clone());
        Ok(())
    }

    async fn receive_message(&self) -> Result<JsonRpcMessage, or_mcp::McpError> {
        self.responses
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| or_mcp::McpError::Transport("no scripted response available".to_owned()))
    }
}

#[tokio::test]
async fn client_lists_tools_over_transport() {
    let transport =
        ScriptedTransport::with_responses(vec![JsonRpcMessage::Success(JsonRpcSuccessResponse {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(1),
            result: serde_json::json!({ "tools": [echo_tool()] }),
        })]);
    let client = NexusClient::new(transport.clone());
    let tools = client.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    let sent = transport.sent.lock().await;
    assert!(matches!(
        &sent[0],
        JsonRpcMessage::Request(JsonRpcRequest { method, .. }) if method == "tools/list"
    ));
}

#[tokio::test]
async fn server_lists_tools_calls_tools_and_returns_tasks() {
    let transport = ScriptedTransport::default();
    let mut server = NexusServer::new(
        transport,
        ServerCard {
            name: "orchustr".to_owned(),
            version: "0.1.2".to_owned(),
            protocol_version: "2025-11-25".to_owned(),
        },
    );
    server
        .register_tool_handler(echo_tool(), |args| async move {
            Ok(serde_json::json!({ "echo": args }))
        })
        .await
        .unwrap();
    server
        .register_task(McpTask {
            id: "task-1".to_owned(),
            status: "completed".to_owned(),
            expires_at: None,
        })
        .await;

    let listed = server
        .handle_message(JsonRpcMessage::Request(JsonRpcRequest {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(1),
            method: "tools/list".to_owned(),
            params: Some(serde_json::json!({})),
        }))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        listed,
        JsonRpcMessage::Success(JsonRpcSuccessResponse { result, .. })
            if result["tools"].as_array().is_some_and(|tools| tools.len() == 1)
    ));

    let called = server
        .handle_message(JsonRpcMessage::Request(JsonRpcRequest {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(2),
            method: "tools/call".to_owned(),
            params: Some(serde_json::json!({ "name": "echo", "arguments": { "value": 7 } })),
        }))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        called,
        JsonRpcMessage::Success(JsonRpcSuccessResponse { result, .. })
            if result["echo"]["value"] == 7
    ));

    let task = server
        .handle_message(JsonRpcMessage::Request(JsonRpcRequest {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(3),
            method: "tasks/get".to_owned(),
            params: Some(serde_json::json!({ "id": "task-1" })),
        }))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        task,
        JsonRpcMessage::Success(JsonRpcSuccessResponse { result, .. })
            if result["status"] == "completed"
    ));
    assert!(server.server_card_json().unwrap().contains("orchustr"));
}

fn echo_tool() -> McpTool {
    McpTool {
        name: "echo".to_owned(),
        description: "Echo arguments".to_owned(),
        input_schema: schemars::json_schema!({ "type": "object" }),
    }
}
