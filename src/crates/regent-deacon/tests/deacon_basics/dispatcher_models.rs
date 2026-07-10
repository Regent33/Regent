//! Model catalog + configured-provider surfaces (model.*, providers.*, mom.run).

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_deacon::Dispatcher;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

#[tokio::test]
async fn dispatcher_model_list_and_set() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(std::sync::Arc::clone(&sm), tx);

    // list exposes the catalog
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "model.list".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let items = v["result"].as_array().unwrap();
    assert!(items.iter().any(|m| m["id"] == "claude-sonnet-4-6"));

    // set switches the active model for new sessions
    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "model.set".into(),
        params: json!({"model": "claude-opus-4-8"}),
        id: Some(json!(2)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["result"]["model"], "claude-opus-4-8");
    assert_eq!(sm.model(), "claude-opus-4-8");
}

#[tokio::test]
async fn dispatcher_model_list_merges_configured_providers() {
    // §A.P1: model.list surfaces configured providers' models as "<provider>/<model>".
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let cfg: regent_deacon::DeaconConfig = serde_json::from_value(json!({
        "providers": {
            "groq": { "kind": "groq", "api_key_env": "X", "models": ["llama-3.3-70b"] }
        }
    }))
    .unwrap();
    let d = Dispatcher::new(sm, tx).with_config(cfg);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "model.list".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let items = v["result"].as_array().unwrap();
    // static catalog still present …
    assert!(items.iter().any(|m| m["id"] == "claude-sonnet-4-6"));
    // … plus the configured provider's model, namespaced.
    assert!(
        items.iter().any(|m| m["id"] == "groq/llama-3.3-70b"),
        "merged provider model"
    );
}

#[tokio::test]
async fn dispatcher_providers_list_returns_configured() {
    // §A CLI: providers.list surfaces the configured map + key presence.
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let cfg: regent_deacon::DeaconConfig = serde_json::from_value(json!({
        "providers": {
            "groq": { "kind": "groq", "api_key_env": "REGENT_TEST_NO_SUCH_KEY", "models": ["llama-3.3-70b"] }
        }
    }))
    .unwrap();
    let d = Dispatcher::new(sm, tx).with_config(cfg);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "providers.list".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let items = v["result"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "groq");
    assert_eq!(items[0]["key_present"], false, "env var is unset");
    assert_eq!(items[0]["models"][0], "llama-3.3-70b");
}

#[tokio::test]
async fn dispatcher_providers_models_merges_config_with_kind_defaults() {
    // Every provider offers the full pickable catalog: the provider's own
    // `models:` entries lead the list, then the KIND's curated defaults follow
    // (deduped) — never blank, never hiding the wider catalog behind one
    // user-configured id. Ollama pointed at ollama.com gets the HOSTED catalog.
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let cfg: regent_deacon::DeaconConfig = serde_json::from_value(json!({
        "providers": {
            // Configured model leads; kind defaults append after it.
            "groq": { "kind": "groq", "api_key_env": "K", "models": ["my-custom-model"] },
            // No models → the anthropic kind's curated defaults fill in.
            "claude": { "kind": "anthropic", "api_key_env": "K", "models": [] },
            // ollama.com base_url → hosted catalog (local ollama stays empty).
            "ollama-cloud": {
                "kind": "ollama", "base_url": "https://ollama.com",
                "api_key_env": "K", "models": ["minimax-m3"]
            }
        }
    }))
    .unwrap();
    let d = Dispatcher::new(sm, tx).with_config(cfg);

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "providers.models".into(),
        params: json!({}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    let map = &v["result"];
    // Configured model first, kind defaults after, no duplicates.
    let groq = map["groq"].as_array().unwrap();
    assert_eq!(groq[0], json!("my-custom-model"));
    assert!(groq.iter().any(|m| m == "llama-3.3-70b-versatile"));
    // Empty config models → the kind catalog appears (non-empty, curated ids).
    let claude = map["claude"].as_array().unwrap();
    assert!(!claude.is_empty(), "kind defaults fill an empty models list");
    assert!(claude.iter().any(|m| m == "claude-opus-4-8"));
    // Hosted ollama: configured model leads and is NOT duplicated by the
    // hosted catalog (minimax-m3 is in both); catalog follows.
    let ollama = map["ollama-cloud"].as_array().unwrap();
    assert_eq!(ollama[0], json!("minimax-m3"));
    assert_eq!(ollama.iter().filter(|m| **m == json!("minimax-m3")).count(), 1);
    assert!(ollama.iter().any(|m| m == "glm-5.2"));
}

#[tokio::test]
async fn dispatcher_providers_test_unknown_is_error() {
    // Offline path: an unknown provider name resolves to no model → -32602,
    // never a network call.
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx).with_config(regent_deacon::DeaconConfig::default());

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "providers.test".into(),
        params: json!({"name": "nope"}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32602);
}

#[tokio::test]
async fn dispatcher_mom_run_unknown_group_is_error() {
    // §B: mom.run on a group that isn't configured → -32602, offline (no provider call).
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let (tx, mut out_rx) = unbounded_channel();
    let d = Dispatcher::new(sm, tx).with_config(regent_deacon::DeaconConfig::default());

    d.handle(regent_deacon::RpcRequest {
        jsonrpc: "2.0".into(),
        method: "mom.run".into(),
        params: json!({"name": "nope", "brief": "hi"}),
        id: Some(json!(1)),
    })
    .await;
    let v: Value = serde_json::from_str(&out_rx.recv().await.unwrap()).unwrap();
    assert_eq!(v["error"]["code"], -32602);
}
