use crate::domain::entities::{JsonRpcMessage, JsonRpcPacket};
use crate::domain::errors::McpError;

pub fn encode_message(message: &JsonRpcMessage) -> Result<String, McpError> {
    serde_json::to_string(message).map_err(|error| McpError::Serialization(error.to_string()))
}

pub fn encode_packet(packet: &JsonRpcPacket) -> Result<String, McpError> {
    serde_json::to_string(packet).map_err(|error| McpError::Serialization(error.to_string()))
}

pub fn decode_message(raw: &str) -> Result<JsonRpcMessage, McpError> {
    match decode_packet(raw)? {
        JsonRpcPacket::Single(message) => Ok(message),
        JsonRpcPacket::Batch(_) => Err(McpError::Protocol(
            "expected a single JSON-RPC message".to_owned(),
        )),
    }
}

pub fn decode_packet(raw: &str) -> Result<JsonRpcPacket, McpError> {
    serde_json::from_str(raw).map_err(|error| McpError::Serialization(error.to_string()))
}

pub fn decode_streamable_body(raw: &str) -> Result<Option<JsonRpcMessage>, McpError> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    if raw.trim_start().starts_with('{') || raw.trim_start().starts_with('[') {
        return decode_packet(raw).map(|packet| match packet {
            JsonRpcPacket::Single(message) => Some(message),
            JsonRpcPacket::Batch(mut messages) => messages.drain(..).next(),
        });
    }

    let mut payload = String::new();
    for line in raw.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            payload.push_str(data.trim());
        }
    }
    if payload.is_empty() {
        Ok(None)
    } else {
        decode_message(&payload).map(Some)
    }
}
