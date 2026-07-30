//! What the AI can SEE follows the folder you picked.
//!
//! `rebind_workspace`'s unit tests cover the refusal path, and
//! `session_sandbox` covers "opening a folder turns the jail on". Neither
//! answers the question that matters after a mis-click: once the session moves
//! from repo A to repo B, is A actually out of reach and B in it?
//!
//! Asked directly on 2026-07-30 — "ensure the file visibility for the ai chat
//! changes too depending on the repo selected". Proven here through a real tool
//! call rather than by inspecting the context, because what the model can read
//! is the only form of the claim worth making.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_kernel::ChatMessage;
use tempfile::TempDir;

fn read_file_call(id: &str, path: &std::path::Path) -> regent_providers::ChatResponse {
    use or_core::TokenUsage;
    let args = serde_json::json!({ "path": path.display().to_string() }).to_string();
    regent_providers::ChatResponse {
        message: ChatMessage::assistant(
            None,
            vec![regent_kernel::ToolCall {
                id: id.into(),
                name: "read_file".into(),
                arguments: args,
            }],
        ),
        usage: TokenUsage::default(),
        finish_reason: Some("tool_calls".into()),
    }
}

/// Every tool message in the session, in order — where a jail refusal lands.
fn tool_results(sm: &regent_deacon::SessionManager, sid: &regent_kernel::SessionId) -> Vec<String> {
    sm.store_handle()
        .get_conversation(sid)
        .expect("conversation reads")
        .into_iter()
        .filter(|m| m.message.role == regent_kernel::Role::Tool)
        .map(|m| m.message.content.unwrap_or_default())
        .collect()
}

#[tokio::test]
async fn rebinding_moves_what_the_model_can_read() {
    let dir = TempDir::new().unwrap();
    let repo_a = dir.path().join("repo-a");
    let repo_b = dir.path().join("repo-b");
    std::fs::create_dir_all(&repo_a).unwrap();
    std::fs::create_dir_all(&repo_b).unwrap();
    let secret_in_a = repo_a.join("only-in-a.txt");
    let file_in_b = repo_b.join("only-in-b.txt");
    std::fs::write(&secret_in_a, "A CONTENTS").unwrap();
    std::fs::write(&file_in_b, "B CONTENTS").unwrap();

    let provider = ScriptedProvider::with(vec![
        // Turn 1, still on repo A: the file there reads fine. The control — a
        // later refusal means nothing unless the same call worked before.
        read_file_call("r1", &secret_in_a),
        ScriptedProvider::text_reply("read A"),
        // Turn 2, after the rebind: the SAME path must now be refused…
        read_file_call("r2", &secret_in_a),
        ScriptedProvider::text_reply("tried A"),
        // …and the new repo's file must be reachable.
        read_file_call("r3", &file_in_b),
        ScriptedProvider::text_reply("read B"),
        // Spare replies. The refused read is a FAILED tool call, and a failed
        // call earns one full-catalog retry (`RetryState::error_recovery_
        // attempted`) — a real recovery path, so the script has to feed it
        // rather than the test pretending turns cost exactly two calls.
        ScriptedProvider::text_reply("spare"),
        ScriptedProvider::text_reply("spare"),
        ScriptedProvider::text_reply("spare"),
    ]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let sid = sm
        .create_session_with_workspace(Some(repo_a.clone()))
        .await
        .unwrap();

    sm.run_turn(&sid, "read the file in A").await.unwrap();
    let before = tool_results(&sm, &sid);
    assert!(
        before.iter().any(|r| r.contains("A CONTENTS")),
        "control: while bound to repo A the model can read A's file — got {before:?}"
    );

    let bound = sm
        .rebind_workspace(&sid, &repo_b)
        .await
        .expect("rebind succeeds")
        .expect("the session is live");
    assert!(
        bound.ends_with("repo-b"),
        "the session reports the new root: {}",
        bound.display()
    );
    assert_eq!(
        sm.workspace_root(&sid).await.as_deref(),
        Some(bound.as_path()),
        "the registry moved too, so a resume re-opens the new folder"
    );

    sm.run_turn(&sid, "read A again").await.unwrap();
    sm.run_turn(&sid, "read the file in B").await.unwrap();
    let after = tool_results(&sm, &sid);
    let fresh = &after[before.len()..];

    // The whole point: the old repo is gone from the model's reach. Asserted on
    // the CONTENT, not on an error string — a refusal that still leaked the
    // bytes would pass a message-shaped assertion.
    assert!(
        !fresh.iter().any(|r| r.contains("A CONTENTS")),
        "repo A must be unreachable after rebinding away from it — got {fresh:?}"
    );
    assert!(
        fresh.iter().any(|r| r.contains("B CONTENTS")),
        "the newly picked repo must be readable — got {fresh:?}"
    );
}
