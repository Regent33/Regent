//! ADR-038 P1/P2: light-profile routing + one-way escalation. The P0/P4
//! catalog-mechanics tests (and `visible_names`) live in tiering.rs.

use crate::helpers::{ScriptedProvider, make_session_manager};
use crate::tiering::visible_names;
use regent_kernel::{ChatMessage, SessionId};
use serde_json::Value;
use tempfile::TempDir;

/// One scripted assistant turn that calls `load_tools` (an escalation
/// trigger), for driving the P2 flow end-to-end.
fn load_tools_call() -> regent_providers::ChatResponse {
    use or_core::TokenUsage;
    regent_providers::ChatResponse {
        message: ChatMessage::assistant(
            None,
            vec![regent_kernel::ToolCall {
                id: "call_1".into(),
                name: "load_tools".into(),
                arguments: r#"{"names":["kanban"]}"#.into(),
            }],
        ),
        usage: TokenUsage::default(),
        finish_reason: Some("tool_calls".into()),
    }
}

async fn budget(sm: &regent_deacon::SessionManager, sid: &SessionId) -> Value {
    sm.context_budget(sid).await.expect("known session")
}

fn segment_names(budget: &Value) -> Vec<String> {
    budget["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect()
}

// A plain chat session routes to the light profile (header leads the prompt,
// agentic tools deferred), then escalates to full EXACTLY ONCE when the model
// reaches for `load_tools` — tagged `cache_reset: "profile"`, never repeated.
#[tokio::test]
async fn chat_routes_light_then_escalates_one_way() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![
        // Turn 1: the model reaches for an agentic tool, then answers.
        load_tools_call(),
        ScriptedProvider::text_reply("loaded"),
        // Turn 2 (escalation applies before this call) and turn 3.
        ScriptedProvider::text_reply("two"),
        ScriptedProvider::text_reply("three"),
    ]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let sid = sm.create_session().await.unwrap();
    let light = budget(&sm, &sid).await;
    assert!(
        segment_names(&light).contains(&"profile_header".to_owned()),
        "chat is born on the light profile (P1)"
    );
    let light_defs = light["tool_defs"]["chars"].as_u64().unwrap();

    sm.run_turn(&sid, "please load kanban").await.unwrap();
    assert_eq!(
        sm.last_turn_cache_reset(&sid).await,
        None,
        "the triggering turn itself still runs light — escalation is next-turn"
    );

    sm.run_turn(&sid, "second").await.unwrap();
    assert_eq!(
        sm.last_turn_cache_reset(&sid).await,
        Some("profile"),
        "the escalated turn is attributed"
    );
    let full = budget(&sm, &sid).await;
    assert!(
        !segment_names(&full).contains(&"profile_header".to_owned()),
        "escalated prompt is the full profile (no header)"
    );
    assert!(
        full["tool_defs"]["chars"].as_u64().unwrap() > light_defs,
        "escalation un-defers the catalog"
    );

    sm.run_turn(&sid, "third").await.unwrap();
    assert_eq!(
        sm.last_turn_cache_reset(&sid).await,
        None,
        "one-way: no second reset, no oscillation"
    );

    // Telemetry: born light, escalated once (the escalation-rate inputs).
    assert_eq!(sm.store_handle().profile_stats(30.0).unwrap(), (1, 1));
}

// The kill-switch: `tools.light_profile = false` restores pre-P1 behavior
// byte-for-byte — no header, full catalog, nothing to escalate.
#[tokio::test]
async fn kill_switch_restores_full_profile_chat() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let tools_cfg = regent_deacon::ToolsConfig {
        light_profile: false,
        ..regent_deacon::ToolsConfig::default()
    };
    let (sm, _rx) = crate::helpers::make_session_manager_with_tools(&dir, provider, tools_cfg);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let sid = sm.create_session().await.unwrap();
    let b = budget(&sm, &sid).await;
    assert!(
        !segment_names(&b).contains(&"profile_header".to_owned()),
        "kill-switch off → full profile, byte-identical to pre-P1"
    );
    assert_eq!(
        sm.store_handle().profile_stats(30.0).unwrap(),
        (0, 0),
        "no light sessions recorded"
    );
}

// Detached background jobs are agentic — never routed light (P1 exception).
#[tokio::test]
async fn detached_background_jobs_stay_full() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("done")]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    sm.run_detached_task("do a thing", |_| {}).await.unwrap();
    assert_eq!(
        sm.store_handle().profile_stats(30.0).unwrap(),
        (0, 0),
        "a background job is born full, not light"
    );
}

// A light-born, never-escalated session RESUMES light: rebuilding it full
// would bust the prompt-prefix cache its stored light bytes own (the tool
// defs would change under it) and leave it counted light-unescalated in the
// telemetry while really running full.
#[tokio::test]
async fn light_session_resumes_light_and_can_still_escalate() {
    let dir = TempDir::new().unwrap();
    let sid = {
        let provider = ScriptedProvider::with(vec![]);
        let (sm, _rx) = make_session_manager(&dir, provider);
        sm.install_admin(regent_deacon::AdminDeps::default());
        sm.create_session().await.unwrap()
    };

    // A fresh manager over the same store — the deacon restarted.
    let provider = ScriptedProvider::with(vec![
        load_tools_call(),
        ScriptedProvider::text_reply("loaded"),
        ScriptedProvider::text_reply("two"),
    ]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());
    sm.resume_session(sid.clone()).await.unwrap();

    let b = budget(&sm, &sid).await;
    assert!(
        segment_names(&b).contains(&"profile_header".to_owned()),
        "a light-born, never-escalated session resumes on the light profile"
    );

    // And the resumed entry still carries the escalation path (P2).
    sm.run_turn(&sid, "please load kanban").await.unwrap();
    sm.run_turn(&sid, "second").await.unwrap();
    assert_eq!(
        sm.last_turn_cache_reset(&sid).await,
        Some("profile"),
        "a resumed light session can still escalate"
    );
    assert_eq!(sm.store_handle().profile_stats(30.0).unwrap(), (1, 1));
}

// Dynamic-retrieval accuracy is mechanical, not hoped-for: EVERY registered
// tool is either resident or named in load_tools for both production surfaces
// (full = Butler, light = main chat). The skills index rides both prompts.
#[tokio::test]
async fn every_registered_tool_is_reachable_in_butler_and_main_chat() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let (full_prompt, full_defs) = sm.fixed_prefix_for(false).await.unwrap();
    let (light_prompt, light_defs) = sm.fixed_prefix_for(true).await.unwrap();

    let registered: std::collections::BTreeSet<_> = sm
        .list_tool_definitions()
        .await
        .unwrap()
        .into_iter()
        .map(|definition| definition.name)
        .collect();
    for (profile, defs) in [
        ("Butler/full", &full_defs),
        ("main-chat/light", &light_defs),
    ] {
        let visible = visible_names(defs);
        let parsed: Value = serde_json::from_str(defs).unwrap();
        let index = parsed
            .as_array()
            .unwrap()
            .iter()
            .find(|definition| definition["name"] == "load_tools")
            .expect("a tiered catalog always carries load_tools")["description"]
            .as_str()
            .unwrap();
        for name in &registered {
            assert!(
                visible.contains(name) || index.contains(name),
                "{name} is unreachable in {profile} (not resident and absent from load_tools)"
            );
        }
    }

    // Skills are discoverable the same way in both profiles: the prompt
    // carries the index, and skill_view stays resident under light.
    assert!(full_prompt.contains("<available_skills>"));
    assert!(light_prompt.contains("<available_skills>"));
    let light_names = visible_names(&light_defs);
    assert!(light_names.contains(&"skill_view".to_owned()));
    assert!(light_names.contains(&"skills_list".to_owned()));
}
