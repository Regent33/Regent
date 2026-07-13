//! Wave 1c harness-skill seam: `code.plan` with a named skill builds its
//! session with the skill body appended to the (stored, frozen) system prompt;
//! an unknown name is a hard error.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_store::Store;
use tempfile::TempDir;

#[tokio::test]
async fn code_plan_with_bundled_skill_appends_body_to_stored_prompt() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("PLAN: one line")]);
    let (sm, _rx) = make_session_manager(&dir, provider);

    let (session_id, plan) = sm
        .code_plan("add a --dry-run flag", Some("ponytail"))
        .await
        .unwrap();
    assert_eq!(plan, "PLAN: one line");

    // Second connection onto the same SQLite file to inspect what was stored.
    let store = Store::open(&dir.path().join("state.db")).unwrap();
    let prompt = store
        .session_system_prompt(&session_id)
        .unwrap()
        .expect("code session stores its prompt");
    assert!(
        prompt.contains("## Active skill: ponytail"),
        "skill overlay header missing"
    );
    assert!(prompt.contains("ladder"), "bundled ponytail body missing");
}

#[tokio::test]
async fn code_plan_with_unknown_skill_is_a_hard_error() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    let err = sm
        .code_plan("task", Some("no-such-skill"))
        .await
        .expect_err("unknown skill must not silently run skill-less");
    assert!(err.to_string().contains("no-such-skill"), "{err}");
}
