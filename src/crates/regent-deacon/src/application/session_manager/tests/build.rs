//! Unit tests for `build` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).
//!
//! The "light defs are strictly smaller than full" and "light prompt bytes
//! == full prompt bytes" acceptance checks (ADR-038 P0(b)) live in
//! `tests/deacon_basics/tiering.rs` instead of here: proving them needs a
//! real `SessionManager` (store, skills library, provider) built from the
//! `make_session_manager`/`ScriptedProvider` fixture that only the
//! integration-test crate has — this unit-test module has no DB/skills
//! fixture of its own. `LIGHT_PINNED`'s own shape is checked here, pure.

use super::super::prompt_lines::{VOICE_SESSION_ENV, voice_flag_enabled};
use super::{LIGHT_PINNED, must_stay_resident, unearned};

// ADR-038 P0(b): the light profile's pinned set must be a strict subset of
// what a real config-pinned session keeps resident — narrower on purpose
// (that's the whole savings) — and must always include the escalation
// trigger so P2's routing (not yet built) has something to route on.
#[test]
fn light_pinned_is_the_minimal_escalation_safe_set() {
    assert!(
        LIGHT_PINNED.contains(&"code_task"),
        "escalation trigger stays visible"
    );
    // Grew from 6 as owner repros proved direct recall/media/skill discovery
    // load-bearing, then to 12 when board tracking became unconditional (owner
    // call: every task request files a card, so `kanban` cannot sit behind a
    // load_tools hop), then to 13 for `delegate_task` — the prompt's
    // parallel-task rule is unreachable behind two hops, and at ~170 tokens the
    // schema is cheap enough to make that a real trade rather than a wish —
    // and to 14 when an owner repro showed computer_use hidden until they
    // reminded the model it existed.
    // Keep a hard ceiling so "light" cannot silently become full — raise it only
    // for a tool the profile genuinely cannot work without.
    assert!(
        LIGHT_PINNED.len() <= 14,
        "the light set stays small (at most 14 pinned tools)"
    );
    for required in [
        "memory",
        "memory_search",
        "session_list",
        "session_search",
        "update_persona",
        "play",
        "computer_use",
        "skills_list",
        // Both are rules the prompt applies UNCONDITIONALLY — a card for every
        // task, one parallel call for several asks. A rule that fires every time
        // cannot depend on a hop weak models skip.
        "kanban",
        "delegate_task",
    ] {
        assert!(
            LIGHT_PINNED.contains(&required),
            "{required} must stay resident on light — an unconditional prompt \
             rule cannot depend on a load_tools hop"
        );
    }
    assert!(
        !LIGHT_PINNED.contains(&"load_tools"),
        "load_tools is auto-injected by the catalog's tiering, never listed here"
    );
}

#[test]
fn voice_sessions_keep_direct_media_and_app_control_tools_resident() {
    assert_eq!(VOICE_SESSION_ENV, "REGENT_VOICE");
    let voice_session = voice_flag_enabled(Some("1"));
    for tool in ["play", "control_app"] {
        assert!(must_stay_resident(tool, voice_session), "{tool}");
        assert!(!must_stay_resident(tool, false), "{tool}");
    }
    assert!(must_stay_resident("kanban", false));
    assert!(
        must_stay_resident("computer_use", false),
        "an enabled automation tool must not disappear behind load_tools in any profile"
    );
    assert!(!must_stay_resident("create_document", true));
}

/// The measured pathology, as a test: a tool used a handful of times across
/// thousands of turns has not earned a schema on every one of them.
///
/// Numbers are the real ones — 4,152 assistant turns in 30 days, and the uses
/// actually recorded against them.
#[test]
fn a_tool_used_twenty_one_times_in_four_thousand_turns_has_not_earned_residency() {
    let turns = 4_152;
    let used: std::collections::HashMap<String, u32> = [
        ("read_file", 1154),
        ("ls", 532),
        ("web_search", 113),
        ("create_document", 21),
        ("world_time", 1),
    ]
    .into_iter()
    .map(|(n, c)| (n.to_owned(), c))
    .collect();
    let names: Vec<String> = used.keys().cloned().collect();

    let out = unearned(names, &used, turns, 0.01);

    assert!(
        out.contains(&"create_document".to_owned()),
        "1,495 tokens on every turn to serve 21 calls: {out:?}"
    );
    assert!(out.contains(&"world_time".to_owned()), "{out:?}");
    for kept in ["read_file", "ls", "web_search"] {
        assert!(
            !out.contains(&kept.to_owned()),
            "{kept} is used constantly and must stay resident: {out:?}"
        );
    }
}

/// The old behaviour is still reachable, and is what `0.0` means: the bar is
/// zero, so only genuinely unused tools defer.
#[test]
fn a_zero_share_is_the_old_any_use_rule() {
    let used: std::collections::HashMap<String, u32> =
        [("seen".to_owned(), 1u32)].into_iter().collect();
    let names = vec!["seen".to_owned(), "never".to_owned()];

    let out = unearned(names, &used, 4_152, 0.0);

    assert_eq!(out, vec!["never".to_owned()], "one use is enough at 0.0");
}

/// Self-calibration is the reason this is a share and not a count: on a quiet
/// store the bar collapses and a new install defers nothing it is using.
#[test]
fn a_quiet_store_defers_nothing_it_actually_uses() {
    let used: std::collections::HashMap<String, u32> =
        [("create_document".to_owned(), 2u32)].into_iter().collect();

    // Same tool, same 2 uses — 40 turns of history vs 4,152.
    let quiet = unearned(vec!["create_document".to_owned()], &used, 40, 0.01);
    let busy = unearned(vec!["create_document".to_owned()], &used, 4_152, 0.01);

    assert!(quiet.is_empty(), "2 uses in 40 turns is heavy use");
    assert_eq!(busy.len(), 1, "2 uses in 4,152 turns is not");
}

/// The boundary, named because getting it wrong reintroduces a known
/// regression: `open_url` sat exactly on the bar against the real store, and it
/// is pinned in the default config precisely because weak models do not make
/// the `load_tools` round trip that deferring it would require.
#[test]
fn a_tool_exactly_on_the_bar_keeps_its_place() {
    let turns = 4_152; // bar = 41
    let used: std::collections::HashMap<String, u32> = [("open_url", 41), ("just_under", 40)]
        .into_iter()
        .map(|(n, c)| (n.to_owned(), c))
        .collect();
    let names: Vec<String> = vec!["open_url".to_owned(), "just_under".to_owned()];

    let out = unearned(names, &used, turns, 0.01);

    assert_eq!(
        out,
        vec!["just_under".to_owned()],
        "reaching the bar is enough; falling below it is not"
    );
}

/// Regression, reported 2026-07-29: asking for a PPTX made the model announce
/// "I'll create the presentation" and then end the turn with no tool call, twice.
///
/// `create_document` was deferred. Measured on the owner's live store the same
/// day: 4,224 assistant turns in the window, so the shipped 1% bar was 42 uses,
/// and `create_document` had 21. Nineteen of the forty-two tools actually used
/// cleared that bar — the other twenty-three, including the only tool that can
/// produce a PPTX, were hidden.
///
/// Deferral is only safe when the model reliably calls `load_tools` to get the
/// schema back. Weak models do not, and the reveal-on-stuck net does not cover
/// them here: it fires on EMPTY content, and a model that answers "I'll do that
/// now" in prose has non-empty content, so the turn ends looking successful.
///
/// So the SHIPPED DEFAULT must not defer a tool the owner genuinely uses. The
/// mechanism and the knob stay; every other test in this file passes 0.01
/// explicitly and none of them pinned what the default itself does.
#[test]
fn the_shipped_default_does_not_defer_a_tool_the_owner_actually_uses() {
    let shipped = crate::domain::config::ToolsConfig::default().auto_tier_min_share;
    // The owner's real distribution on the day this was reported.
    let turns = 4_224;
    let used: std::collections::HashMap<String, u32> = [
        ("create_document".to_owned(), 21_u32), // 6.2M tokens of residency for 21 calls
        ("vision_analyze".to_owned(), 22),
        ("control_app".to_owned(), 10),
        ("read_document".to_owned(), 4),
    ]
    .into_iter()
    .collect();

    let deferred = unearned(used.keys().cloned(), &used, turns, shipped);

    assert!(
        deferred.is_empty(),
        "the shipped default hid tools the owner uses, which is how the PPTX ask \
         dead-ended: {deferred:?} (bar was {} uses of {turns} turns)",
        (f64::from(turns) * shipped).floor()
    );
    // Genuinely unused tools must still defer — the cost finding stands, and a
    // default of "never defer anything" would throw the mechanism away.
    let never: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    assert_eq!(
        unearned(vec!["world_time".to_owned()], &never, turns, shipped),
        vec!["world_time".to_owned()],
        "a tool with no recorded use at all must still defer"
    );
}
