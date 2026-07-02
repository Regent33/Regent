use crate::domain::entities::{JsonRpcMessage, JsonRpcPacket};
use crate::domain::errors::McpError;
use crate::infra::jsonrpc::{decode_packet, encode_packet};

#[derive(Debug, Clone, Default)]
pub struct JsonRpcOrchestrator;

impl JsonRpcOrchestrator {
    pub fn encode(&self, message: JsonRpcMessage) -> Result<String, McpError> {
        let span = tracing::info_span!(
            "mcp.encode_jsonrpc",
            otel.name = "mcp.encode_jsonrpc",
            status = tracing::field::Empty,
        );
        let _guard = span.enter();
        let result = encode_packet(&JsonRpcPacket::Single(message));
        span.record("status", if result.is_ok() { "success" } else { "failure" });
        result
    }

    pub fn decode(&self, raw: &str) -> Result<JsonRpcPacket, McpError> {
        let span = tracing::info_span!(
            "mcp.decode_jsonrpc",
            otel.name = "mcp.decode_jsonrpc",
            status = tracing::field::Empty,
        );
        let _guard = span.enter();
        let result = decode_packet(raw);
        span.record("status", if result.is_ok() { "success" } else { "failure" });
        result
    }
}
