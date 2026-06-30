//! JSON-RPC 2.0 wire types (the daemon's request/response/notification
//! envelopes) and response helpers. Config schema lives in `domain::config`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── JSON-RPC 2.0 types ───────────────────────────────────────────────────────

/// Inbound message from the client (request or client-notification when id
/// is absent).
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub id: Option<Value>,
}

/// Outbound response (always has an id matching the request).
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    #[serde(flatten)]
    pub outcome: RpcOutcome,
    pub id: Value,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RpcOutcome {
    Ok { result: Value },
    Err { error: RpcErrorBody },
}

#[derive(Debug, Serialize, Clone)]
pub struct RpcErrorBody {
    pub code: i32,
    pub message: String,
}

/// Server-to-client notification (no id — client never responds to this).
#[derive(Debug, Clone, Serialize)]
pub struct RpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    pub params: Value,
}

impl RpcNotification {
    #[must_use]
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        }
    }
}

// ── Response helpers ─────────────────────────────────────────────────────────

#[must_use]
pub fn ok_response(id: Option<Value>, result: Value) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        outcome: RpcOutcome::Ok { result },
        id: id.unwrap_or(Value::Null),
    }
}

#[must_use]
pub fn err_response(id: Option<Value>, code: i32, message: impl Into<String>) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        outcome: RpcOutcome::Err {
            error: RpcErrorBody {
                code,
                message: message.into(),
            },
        },
        id: id.unwrap_or(Value::Null),
    }
}
