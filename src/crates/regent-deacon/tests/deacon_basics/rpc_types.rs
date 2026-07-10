//! JSON-RPC type round-trips.

use regent_deacon::RpcNotification;
use regent_deacon::domain::entities::{err_response, ok_response};
use serde_json::{Value, json};

#[test]
fn rpc_request_round_trips() {
    let raw = r#"{"jsonrpc":"2.0","method":"health","params":{},"id":1}"#;
    let req: regent_deacon::RpcRequest = serde_json::from_str(raw).unwrap();
    assert_eq!(req.method, "health");
    assert_eq!(req.id, Some(json!(1)));
}

#[test]
fn ok_response_serialises_correctly() {
    let resp = ok_response(Some(json!(42)), json!({"status": "ok"}));
    let s = serde_json::to_string(&resp).unwrap();
    let v: Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], 42);
    assert_eq!(v["result"]["status"], "ok");
    assert!(v.get("error").is_none());
}

#[test]
fn err_response_serialises_correctly() {
    let resp = err_response(Some(json!(1)), -32601, "Method not found");
    let v: Value = serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32601);
    assert!(v.get("result").is_none());
}

#[test]
fn notification_has_no_id_field() {
    let n = RpcNotification::new("turn.started", json!({"session_id": "x"}));
    let v: Value = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
    assert!(v.get("id").is_none());
    assert_eq!(v["method"], "turn.started");
}

#[test]
fn rpc_request_without_id_is_notification() {
    let raw = r#"{"jsonrpc":"2.0","method":"ping","params":{}}"#;
    let req: regent_deacon::RpcRequest = serde_json::from_str(raw).unwrap();
    assert!(req.id.is_none());
}
