//! Tests for the additive compatibility facts. These assert the DERIVED values
//! (not source text): the prompt schema is parsed from the live system prompt,
//! the store/config schemas come from their real constants, and the version is
//! no longer the historically hard-coded string.

use super::{
    CALL_PROTOCOL, DEACON_RPC_PROTOCOL, VERSION, capabilities, components, prompt_schema, protocols,
};

#[test]
fn version_is_the_real_package_version_not_the_old_hardcode() {
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    assert_ne!(VERSION, "0.1.0", "the drifted hard-code must be gone");
}

#[test]
fn prompt_schema_is_parsed_from_the_live_system_prompt() {
    // Derived from `regent_agent::SYSTEM_PROMPT`'s marker, so it tracks the
    // prompt automatically. It must be a real number, not absent.
    let n = prompt_schema().expect("system prompt carries a schema marker");
    assert!(n >= 1);
    // Cross-check against the marker constant the agent crate ships.
    let expected: u32 = regent_agent::SYSTEM_PROMPT
        .lines()
        .next()
        .unwrap()
        .strip_prefix("regent-prompt-schema:v")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(n, expected);
}

#[test]
fn protocols_report_the_real_schema_constants() {
    let p = protocols();
    assert_eq!(p["deacon_rpc"], DEACON_RPC_PROTOCOL);
    assert_eq!(p["call"], CALL_PROTOCOL);
    assert_eq!(
        p["store_schema"],
        regent_store::infra::schema::SCHEMA_VERSION
    );
    assert_eq!(
        p["config_schema"],
        crate::domain::config::CURRENT_CONFIG_VERSION
    );
    assert_eq!(p["prompt_schema"], prompt_schema().unwrap());
}

#[test]
fn components_and_capabilities_are_present_and_additive() {
    let c = components();
    assert_eq!(c["deacon"], VERSION);
    assert!(
        c.get("cli").is_none(),
        "the deacon cannot know its caller's version"
    );
    // The capability token new clients feature-detect for `update.status`.
    let caps = capabilities();
    assert!(
        caps.as_array()
            .unwrap()
            .iter()
            .any(|v| v == "update.status.v1")
    );
}
