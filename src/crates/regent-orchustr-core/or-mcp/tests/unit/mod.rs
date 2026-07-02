use or_mcp::{
    JsonRpcErrorDetail, JsonRpcId, JsonRpcMessage, JsonRpcNotification, JsonRpcOrchestrator,
    JsonRpcPacket, JsonRpcRequest, JsonRpcSuccessResponse, McpError,
};

mod client_server;
mod known_servers;
mod multi_client;

#[test]
fn encode_and_decode_single_message_round_trip() {
    let message = JsonRpcMessage::Request(JsonRpcRequest {
        jsonrpc: "2.0".to_owned(),
        id: JsonRpcId::Number(1),
        method: "tools/list".to_owned(),
        params: None,
    });
    let encoded = JsonRpcOrchestrator.encode(message.clone()).unwrap();
    let decoded = JsonRpcOrchestrator.decode(&encoded).unwrap();
    assert_eq!(decoded, JsonRpcPacket::Single(message));
}

#[test]
fn decode_batch_packet_round_trip() {
    let raw = serde_json::to_string(&JsonRpcPacket::Batch(vec![
        JsonRpcMessage::Notification(JsonRpcNotification {
            jsonrpc: "2.0".to_owned(),
            method: "initialized".to_owned(),
            params: None,
        }),
        JsonRpcMessage::Success(JsonRpcSuccessResponse {
            jsonrpc: "2.0".to_owned(),
            id: JsonRpcId::Number(1),
            result: serde_json::json!({"ok": true}),
        }),
    ]))
    .unwrap();
    let decoded = JsonRpcOrchestrator.decode(&raw).unwrap();
    assert!(matches!(decoded, JsonRpcPacket::Batch(messages) if messages.len() == 2));
}

#[test]
fn decode_reports_invalid_payloads() {
    let result = JsonRpcOrchestrator.decode("{not json}");
    assert!(matches!(result, Err(McpError::Serialization(_))));
    let _ = JsonRpcErrorDetail {
        code: -32600,
        message: "invalid request".to_owned(),
    };
}
