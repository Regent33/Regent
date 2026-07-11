//! SPL P4 (§3.5): adaptive tool tiering — residency is earned by usage — and
//! the catalog-size acceptance gate. The behavioral risk (the model not
//! *realizing* it needs a deferred tool) is covered by the post-ship eval;
//! these tests prove the mechanics: unused tools defer, pinned and
//! recently-used tools stay resident, and the default model-facing catalog
//! fits the ≤1.5k-token ceiling.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_kernel::{ChatMessage, SessionId};
use serde_json::{Value, json};
use tempfile::TempDir;

/// The model-facing tool names from `fixed_prefix`'s serialized definitions.
fn visible_names(defs_json: &str) -> Vec<String> {
    let v: Value = serde_json::from_str(defs_json).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap().to_owned())
        .collect()
}

/// Wire-shape token estimate (chars/4) of one definition, matching
/// `token_budget.rs` so numbers are comparable across the two files.
fn wire_tokens(def: &Value) -> usize {
    json!({
        "name": def["name"],
        "description": def["description"],
        "input_schema": def["parameters"],
    })
    .to_string()
    .chars()
    .count()
    .div_ceil(4)
}

// A fresh store has no usage → every unpinned tool defers; pinned tools and
// the load_tools loader stay; the model-facing catalog fits the P4 ceiling.
#[tokio::test]
async fn fresh_store_defers_unpinned_and_catalog_fits_the_ceiling() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let (_prompt, defs_json) = sm.fixed_prefix().await.unwrap();
    let names = visible_names(&defs_json);

    for pinned in ["read_file", "terminal", "web_search", "memory_search"] {
        assert!(
            names.contains(&pinned.to_owned()),
            "{pinned} stays resident"
        );
    }
    assert!(names.contains(&"load_tools".to_owned()), "loader present");
    for unused in ["apply_patch", "session_search", "camera_capture"] {
        assert!(
            !names.contains(&unused.to_owned()),
            "{unused} has no recorded use — deferred"
        );
    }

    // Acceptance (proposal §6, P4): catalog block ≤1.5k tokens with defaults.
    let v: Value = serde_json::from_str(&defs_json).unwrap();
    let total: usize = v.as_array().unwrap().iter().map(wire_tokens).sum();
    assert!(
        total <= 1_500,
        "model-facing catalog is {total} tokens (> 1.5k): {names:?}"
    );
}

// A tool invoked inside the 30-day window earns residency: its schema is back
// in the catalog at the next session build, unprompted.
#[tokio::test]
async fn recorded_use_promotes_a_tool_back_into_the_catalog() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    // A recorded apply_patch invocation (the messages ledger IS the counter).
    let sid = SessionId::generate();
    sm.store_handle()
        .create_session(&sid, "deacon", None, None, None)
        .unwrap();
    sm.store_handle()
        .append_message(
            &sid,
            &ChatMessage::tool_result("call_1", "apply_patch", "{\"ok\":true}"),
            None,
            None,
        )
        .unwrap();

    let (_prompt, defs_json) = sm.fixed_prefix().await.unwrap();
    let names = visible_names(&defs_json);
    assert!(
        names.contains(&"apply_patch".to_owned()),
        "usage earned residency: {names:?}"
    );
    // Still-unused peers stay deferred.
    assert!(!names.contains(&"session_search".to_owned()));
}
