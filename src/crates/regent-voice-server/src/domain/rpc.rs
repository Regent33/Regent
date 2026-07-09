//! JSON-RPC line routing for the deacon's stdio stream — the pure demux
//! behind the agent brain. Port of web_call.py's `_classify` (its `__main__`
//! self-checks are the tests below).

use serde_json::Value;

/// What one incoming JSON-RPC line means to the call loop. Streamed events
/// carry their `session_id` (empty when absent) so the call can ignore other
/// sessions' traffic — a detached background job or cron turn streaming
/// through the same deacon must not be spoken into the call.
#[derive(Debug, PartialEq)]
pub enum RpcEvent {
    /// A response to a request we sent, matched by id.
    Response(i64),
    /// A streamed reply fragment (`message.delta`): (session_id, text).
    Delta(String, String),
    /// The final assembled reply (`message.complete`) — used when the
    /// provider didn't stream: (session_id, reply).
    Reply(String, String),
    /// The turn ended (`turn.complete` / `turn.interrupted`):
    /// (session_id, error if any).
    End(String, Option<String>),
    /// Anything else (notifications the call doesn't care about).
    Ignore,
}

/// Route one parsed JSON-RPC line.
#[must_use]
pub fn classify(msg: &Value) -> RpcEvent {
    if let Some(id) = msg.get("id").and_then(Value::as_i64)
        && (msg.get("result").is_some() || msg.get("error").is_some())
    {
        return RpcEvent::Response(id);
    }
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match msg.get("method").and_then(Value::as_str) {
        Some("message.delta") => RpcEvent::Delta(text("session_id"), text("text")),
        Some("message.complete") => RpcEvent::Reply(text("session_id"), text("reply")),
        Some("turn.complete" | "turn.interrupted") => RpcEvent::End(
            text("session_id"),
            params
                .get("error")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        ),
        _ => RpcEvent::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn routes_every_line_kind_like_the_python_self_check() {
        let c = |v: Value| classify(&v);
        assert_eq!(
            c(json!({"jsonrpc": "2.0", "id": 1, "result": {"session_id": "s1"}})),
            RpcEvent::Response(1)
        );
        assert_eq!(
            c(json!({"jsonrpc": "2.0", "id": 2, "error": {"code": -1}})),
            RpcEvent::Response(2)
        );
        assert_eq!(
            c(json!({"method": "message.delta", "params": {"session_id": "s1", "text": "Hi"}})),
            RpcEvent::Delta("s1".into(), "Hi".into())
        );
        // No session_id (older deacon) → empty sid, still routed.
        assert_eq!(
            c(json!({"method": "message.delta", "params": {"text": "Hi"}})),
            RpcEvent::Delta(String::new(), "Hi".into())
        );
        assert_eq!(
            c(json!({"method": "message.complete", "params": {"reply": "Hi there"}})),
            RpcEvent::Reply(String::new(), "Hi there".into())
        );
        assert_eq!(
            c(json!({"method": "turn.complete", "params": {}})),
            RpcEvent::End(String::new(), None)
        );
        assert_eq!(
            c(json!({"method": "turn.interrupted", "params": {"error": "x"}})),
            RpcEvent::End(String::new(), Some("x".into()))
        );
        assert_eq!(
            c(json!({"method": "turn.started", "params": {}})),
            RpcEvent::Ignore
        );
    }

    #[test]
    fn a_request_with_id_but_no_result_is_not_a_response() {
        // The deacon echoes requests? No — but a method call WITH an id (our
        // own writes never come back) must not be misread as a response.
        let v = json!({"jsonrpc": "2.0", "id": 3, "method": "prompt.submit", "params": {}});
        assert_eq!(classify(&v), RpcEvent::Ignore);
    }
}
