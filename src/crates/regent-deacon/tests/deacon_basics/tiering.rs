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

    for pinned in [
        "read_file",
        "terminal",
        "web_search",
        "memory_search",
        "session_search",
        "current_time",
        "play",
        "code_task",
        "apply_patch",
    ] {
        assert!(
            names.contains(&pinned.to_owned()),
            "{pinned} stays resident"
        );
    }
    assert!(names.contains(&"load_tools".to_owned()), "loader present");
    for unused in ["memory", "background_task", "video_analyze"] {
        assert!(
            !names.contains(&unused.to_owned()),
            "{unused} has no recorded use — deferred"
        );
    }

    // Acceptance ceiling. P4's proposal target was 1.5k with a minimal pinned
    // set; the user then mandated (2026-07-11) that recall, time, web-fetch,
    // skills loaders, and the code_task router never hide behind load_tools —
    // that richer always-on set measures ~2.1k. 2026-07-16: create_document +
    // the 10 everyday tools shipped deferred, growing the load_tools index by
    // ~190 tokens (11 entries × name + 60-char hook), followed by the native
    // document/everyday surface, brought the measured catalog to 2.61k.
    // 2026-07-18: camera_capture (137) + vision_analyze (158) moved from
    // deferred to pinned so a Butler "can you see me?" turn is callable on
    // turn 1 — deferred, they forced the weak driver into a reasoning-only
    // dead-end that fires reveal_all_deferred and busts the tier0 prompt-prefix
    // cache for the rest of the vision exchange. Net +~250 (full schemas in,
    // their load_tools hooks out) puts the measured catalog at 2.86k — a
    // one-time first-turn cost that then rides the STABLE cached prefix (no
    // mid-session reveal, no repeated cache bust). This gate stops regression
    // from HERE.
    let v: Value = serde_json::from_str(&defs_json).unwrap();
    let total: usize = v.as_array().unwrap().iter().map(wire_tokens).sum();
    assert!(
        total <= 2_950,
        "model-facing catalog is {total} tokens (> 2.95k): {names:?}"
    );
}

// The `tools.deferred` defaults name tools by bare string — nothing else ties
// them to the registry, so a tool rename would silently stop deferring it
// (quietly regressing the token budget). Pin the core-catalog-registered
// entries to reality; the rest are deacon-wired and exercised above.
#[test]
fn default_deferred_names_match_registered_core_tools() {
    let registered: std::collections::BTreeSet<String> = regent_tools::core_catalog()
        .definitions()
        .into_iter()
        .map(|d| d.name)
        .collect();
    let deferred = regent_deacon::ToolsConfig::default().deferred;
    for name in [
        "create_document",
        "calc",
        "convert",
        "date_calc",
        "dictionary",
        "qr_code",
        "random_gen",
        "reminder",
        "sun_moon",
        "weather",
        "world_time",
        "image_generation",
        "video_analyze",
        "control_app",
        "read_document",
    ] {
        assert!(
            deferred.iter().any(|d| d == name),
            "'{name}' fell out of the deferred defaults"
        );
        assert!(
            registered.contains(name),
            "deferred default '{name}' is not a registered core tool — renamed?"
        );
    }
}

// A tool invoked inside the 30-day window earns residency: its schema is back
// in the catalog at the next session build, unprompted.
#[tokio::test]
async fn recorded_use_promotes_a_tool_back_into_the_catalog() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    // A recorded memory-tool invocation (the messages ledger IS the counter).
    let sid = SessionId::generate();
    sm.store_handle()
        .create_session(&sid, "deacon", None, None, None)
        .unwrap();
    sm.store_handle()
        .append_message(
            &sid,
            &ChatMessage::tool_result("call_1", "memory", "{\"ok\":true}"),
            None,
            None,
        )
        .unwrap();

    let (_prompt, defs_json) = sm.fixed_prefix().await.unwrap();
    let names = visible_names(&defs_json);
    assert!(
        names.contains(&"memory".to_owned()),
        "usage earned residency: {names:?}"
    );
    // Still-unused peers stay deferred.
    assert!(!names.contains(&"background_task".to_owned()));
}

// ADR-038 P0(b): `fixed_prefix_for` renders the `light` candidate profile —
// everything outside its minimal pinned set defers, regardless of use
// history — without touching a real session's catalog. These use the same
// `make_session_manager`/`ScriptedProvider` fixture as the tiering tests
// above (the unit-test module in `session_manager/build_tests.rs` has no DB/
// skills/provider fixture of its own to build a real catalog from).
#[tokio::test]
async fn light_profile_defs_are_strictly_smaller_than_full() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let (_full_prompt, full_defs) = sm.fixed_prefix_for(false).await.unwrap();
    let (_light_prompt, light_defs) = sm.fixed_prefix_for(true).await.unwrap();

    assert!(
        light_defs.len() < full_defs.len(),
        "light defs ({}) must be strictly smaller than full defs ({})",
        light_defs.len(),
        full_defs.len()
    );
    let light_names = visible_names(&light_defs);
    for pinned in [
        "memory_search",
        "session_search",
        "current_time",
        "skill_view",
    ] {
        assert!(
            light_names.contains(&pinned.to_owned()),
            "{pinned} stays resident in light: {light_names:?}"
        );
    }
    // The config-level safety valve (read_file/terminal/…) is NOT the light
    // profile's pinned set — those defer under light even though they never
    // defer under full.
    for full_only_pinned in ["read_file", "terminal", "web_search", "apply_patch"] {
        assert!(
            !light_names.contains(&full_only_pinned.to_owned()),
            "{full_only_pinned} should defer under light: {light_names:?}"
        );
    }
}

// P0 is measurement-only: the light candidate's SYSTEM_PROMPT + ledger bytes
// must be byte-identical to full's — only the tool-definitions payload
// shrinks. A regression here means `light` silently started trimming a
// protected block, which ADR-038 explicitly forbids.
#[tokio::test]
async fn light_profile_prompt_is_the_header_plus_full_bytes() {
    let dir = TempDir::new().unwrap();
    let provider = ScriptedProvider::with(vec![]);
    let (sm, _rx) = make_session_manager(&dir, provider);
    sm.install_admin(regent_deacon::AdminDeps::default());

    let (full_prompt, _full_defs) = sm.fixed_prefix_for(false).await.unwrap();
    let (light_prompt, _light_defs) = sm.fixed_prefix_for(true).await.unwrap();

    // ADR-038 P1: the two profiles must diverge in the FIRST line (implicit-
    // cache providers route on a hash of ~the first 256 tokens) and nowhere
    // else in the prompt — light is exactly the one-line header + full's
    // bytes, so the protected blocks stay verbatim in both.
    assert_eq!(
        light_prompt,
        format!("profile: light\n\n{full_prompt}"),
        "light must be the Tier-0 profile header + full's exact bytes"
    );
}

// ── ADR-038 P1/P2: light-profile routing + one-way escalation ──────────────

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

    sm.run_detached_task("do a thing").await.unwrap();
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
