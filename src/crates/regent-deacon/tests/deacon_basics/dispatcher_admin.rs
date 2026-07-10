//! Admin surfaces: the in-process `regent` tool routing, config.get, cron CRUD,
//! and commands.list.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_deacon::Dispatcher;
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

// The in-process `regent` admin tool routes through this: it must reach real
// handlers once installed, refuse turn/session-lifecycle methods, and report
// when the composition root hasn't installed the self-handle.
#[tokio::test]
async fn run_admin_command_routes_and_refuses_lifecycle() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    // Not installed yet → clear refusal (no panic, no hang).
    let err = sm
        .run_admin_command("model.get", json!({}))
        .await
        .unwrap_err();
    assert!(err.contains("not installed"), "got: {err}");

    sm.install_admin(regent_deacon::AdminDeps::default());

    // Happy path: forwards to the live model.get handler.
    let result = sm.run_admin_command("model.get", json!({})).await.unwrap();
    assert_eq!(result["model"], "scripted");

    // Turn/session lifecycle is off-limits to the agent.
    let err = sm
        .run_admin_command("prompt.submit", json!({}))
        .await
        .unwrap_err();
    assert!(err.contains("live turn/session"), "got: {err}");
}

#[tokio::test]
async fn dispatcher_config_get_round_trips_the_loaded_config() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let cfg = regent_deacon::DeaconConfig::default();
    let d = Dispatcher::new(sm, tx).with_config(cfg);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "config.get".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["_config_version"], 1);
    assert_eq!(v["result"]["cron"]["tick_interval_secs"], 30);
}

#[tokio::test]
async fn dispatcher_cron_add_list_remove_round_trip() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let repo: Arc<dyn regent_cron::JobRepository> =
        Arc::new(regent_cron::FsJobRepository::new(dir.path().join("cron")).unwrap());
    let d = Dispatcher::new(sm, tx).with_cron(repo);

    // add
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "cron.add".into(),
        params: json!({"name": "report", "schedule": "30m", "prompt": "write the report"}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let job_id = v["result"]["id"].as_str().unwrap().to_owned();
    assert!(job_id.starts_with("job_"));

    // list
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "cron.list".into(),
        params: json!({}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let jobs = v["result"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["name"], "report");

    // remove
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "cron.remove".into(),
        params: json!({"id": job_id}),
        id: Some(json!(3)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["removed"], true);

    // bad schedule is a -32602
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "cron.add".into(),
        params: json!({"name": "x", "schedule": "tuesday", "prompt": "y"}),
        id: Some(json!(4)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32602);
}

#[tokio::test]
async fn dispatcher_commands_list_is_non_empty() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "commands.list".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;

    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let items = v["result"].as_array().unwrap();
    assert!(!items.is_empty());

    // Full surface, not just the five chat controls: /learn (skills.create) is
    // present and marked executable over RPC, while a terminal-only command
    // (doctor) is present but marked non-executable so the UI can explain it.
    let find = |name: &str| items.iter().find(|c| c["name"] == name);
    let learn = find("learn").expect("learn command present");
    assert_eq!(learn["executable"], true, "learn runs via RPC (skills.create)");
    assert_eq!(
        find("doctor").expect("doctor present")["executable"],
        false,
        "doctor has no RPC path"
    );
    // Every row carries name + description + the additive executable flag.
    for c in items {
        assert!(c["name"].is_string(), "name: {c}");
        assert!(c["description"].is_string(), "description: {c}");
        assert!(c["executable"].is_boolean(), "executable flag: {c}");
    }
}
