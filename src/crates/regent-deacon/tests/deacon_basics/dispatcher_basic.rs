//! Dispatcher routing basics: health, unknown methods, session + memory +
//! model/skills read surfaces.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_deacon::Dispatcher;
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

#[tokio::test]
async fn dispatcher_health_returns_ok() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "health".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;

    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["status"], "ok");
    assert_eq!(v["id"], 1);
}

#[tokio::test]
async fn dispatcher_unknown_method_returns_minus_32601() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "no.such.method".into(),
        params: json!({}),
        id: Some(json!(99)),
    })
    .await;

    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32601);
    assert_eq!(v["id"], 99);
}

#[tokio::test]
async fn dispatcher_memory_pending_and_reject() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    // empty approval queue
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "memory.pending".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert!(v["result"].as_array().unwrap().is_empty());

    // rejecting an unknown id is a clean no-op
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "memory.reject".into(),
        params: json!({"id": "pw_nope"}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["removed"], false);
}

#[tokio::test]
async fn dispatcher_session_create_then_list() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(Arc::clone(&sm), tx);

    // session.create
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "session.create".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let sid = v["result"]["session_id"].as_str().unwrap().to_owned();
    assert!(sid.starts_with("sess_"));

    // session.list
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "session.list".into(),
        params: json!({}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let items = v["result"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["session_id"].as_str().unwrap(), sid);
}

#[tokio::test]
async fn dispatcher_model_get_and_skills_list() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "model.get".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["model"], "scripted");

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "skills.list".into(),
        params: json!({}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert!(v["result"].is_array());
}
