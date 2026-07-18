//! Unit tests for `catalog` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use crate::domain::contracts::DenyAll;
use async_trait::async_trait;
use serde_json::json;

struct Boom;

#[async_trait]
impl ToolExecutor for Boom {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        Err(RegentError::Tool {
            tool: "boom".into(),
            message: "kapow".into(),
        })
    }
}

fn definition(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: "test".into(),
        parameters: json!({"type": "object"}),
        toolset: "test".into(),
    }
}

fn ctx() -> ToolContext {
    ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
}

#[test]
fn reveal_all_deferred_activates_hidden_tools_for_the_next_definitions() {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(definition("alpha"), Arc::new(Boom))
        .unwrap();
    catalog
        .register(definition("beta"), Arc::new(Boom))
        .unwrap();
    catalog.defer(&["beta".to_owned()]).unwrap();
    // Deferred: beta's schema is withheld; the load_tools loader stands in.
    let names: Vec<String> = catalog.definitions().into_iter().map(|d| d.name).collect();
    assert!(!names.contains(&"beta".to_owned()), "beta starts deferred");
    assert!(names.contains(&"load_tools".to_owned()), "loader present");
    // Reveal-all activates every deferred tool → beta now lists.
    assert_eq!(catalog.reveal_all_deferred(), 1);
    let names: Vec<String> = catalog.definitions().into_iter().map(|d| d.name).collect();
    assert!(names.contains(&"beta".to_owned()), "beta revealed");
    // Idempotent: a second reveal activates nothing new.
    assert_eq!(catalog.reveal_all_deferred(), 0);
}

#[tokio::test]
async fn unknown_tool_and_bad_args_return_error_json() {
    let catalog = ToolCatalog::new();
    let out = catalog.dispatch("nope", "{}", &ctx()).await;
    assert!(out.contains("unknown tool"));
    let mut catalog = ToolCatalog::new();
    catalog
        .register(definition("boom"), Arc::new(Boom))
        .unwrap();
    let out = catalog.dispatch("boom", "not json", &ctx()).await;
    assert!(out.contains("invalid tool arguments"));
}

#[tokio::test]
async fn executor_errors_are_wrapped_not_thrown() {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(definition("boom"), Arc::new(Boom))
        .unwrap();
    let out = catalog.dispatch("boom", "{}", &ctx()).await;
    let value: Value = serde_json::from_str(&out).unwrap();
    assert!(value["error"].as_str().unwrap().contains("kapow"));
}

struct Echo;

#[async_trait]
impl ToolExecutor for Echo {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        Ok("\"ok\"".into())
    }
}

/// Deferred tools: schema withheld until loaded, still executable, and
/// `load_tools` returns the schema + activates for the next turn.
#[tokio::test]
async fn deferred_tools_hide_until_loaded_but_stay_executable() {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(definition("rare_tool"), Arc::new(Echo))
        .unwrap();
    catalog
        .register(definition("core_tool"), Arc::new(Echo))
        .unwrap();
    catalog
        .defer(&["rare_tool".into(), "no_such".into()])
        .unwrap();

    let names: Vec<_> = catalog.definitions().into_iter().map(|d| d.name).collect();
    assert!(names.contains(&"core_tool".to_owned()));
    assert!(names.contains(&"load_tools".to_owned()));
    assert!(
        !names.contains(&"rare_tool".to_owned()),
        "deferred schema withheld"
    );

    // load_tools returns the schema and activates it.
    let out = catalog
        .dispatch("load_tools", r#"{"names":["rare_tool","nope"]}"#, &ctx())
        .await;
    assert!(out.contains("rare_tool") && out.contains("nope"));
    let names: Vec<_> = catalog.definitions().into_iter().map(|d| d.name).collect();
    assert!(
        names.contains(&"rare_tool".to_owned()),
        "activated after load"
    );

    // Direct calls to a deferred tool always execute (forgiving path).
    let mut catalog2 = ToolCatalog::new();
    catalog2
        .register(definition("rare_tool"), Arc::new(Echo))
        .unwrap();
    catalog2.defer(&["rare_tool".into()]).unwrap();
    assert_eq!(catalog2.dispatch("rare_tool", "{}", &ctx()).await, "\"ok\"");
    let names: Vec<_> = catalog2.definitions().into_iter().map(|d| d.name).collect();
    assert!(
        names.contains(&"rare_tool".to_owned()),
        "direct call activates"
    );
}

/// A near-miss `load_tools` name self-corrects: the unknown entry carries
/// `did_you_mean` suggestions matched by substring either direction, so the
/// model's next call lands (the dynamic-retrieval accuracy loop).
#[tokio::test]
async fn load_tools_suggests_close_matches_for_unknown_names() {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(definition("web_search"), Arc::new(Echo))
        .unwrap();
    catalog
        .register(definition("read_document"), Arc::new(Echo))
        .unwrap();
    catalog
        .defer(&["web_search".into(), "read_document".into()])
        .unwrap();

    // "search" (partial), "READ_DOCUMENT_TOOL" (superset, wrong case).
    let out = catalog
        .dispatch(
            "load_tools",
            r#"{"names":["search","READ_DOCUMENT_TOOL","zzz"]}"#,
            &ctx(),
        )
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let unknown = v["unknown"].as_array().unwrap();
    let suggestions_for = |ask: &str| -> Vec<String> {
        unknown
            .iter()
            .find(|u| u["name"] == ask)
            .expect("unknown entry present")["did_you_mean"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(suggestions_for("search"), vec!["web_search"]);
    assert_eq!(suggestions_for("READ_DOCUMENT_TOOL"), vec!["read_document"]);
    assert!(suggestions_for("zzz").is_empty(), "no false suggestions");
}

#[test]
fn duplicate_registration_rejected_and_order_deterministic() {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(definition("zeta"), Arc::new(Boom))
        .unwrap();
    catalog
        .register(definition("alpha"), Arc::new(Boom))
        .unwrap();
    assert!(
        catalog
            .register(definition("alpha"), Arc::new(Boom))
            .is_err()
    );
    let names: Vec<_> = catalog.definitions().into_iter().map(|d| d.name).collect();
    assert_eq!(names, vec!["alpha", "zeta"]);
}

/// A restriction allow-list is an explicit "these must work": tools that
/// survive `restrict_to` become visible even if they were deferred (a
/// restricted catalog rarely keeps `load_tools`, so a still-deferred
/// survivor would be invisible AND unloadable — the CodePlan hole).
#[tokio::test]
async fn restrict_to_undefers_the_survivors() {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(definition("keep"), Arc::new(Echo))
        .unwrap();
    catalog
        .register(definition("drop"), Arc::new(Echo))
        .unwrap();
    catalog.defer(&["keep".into(), "drop".into()]).unwrap();
    catalog.restrict_to(&["keep".into()]);
    let names: Vec<_> = catalog.definitions().into_iter().map(|d| d.name).collect();
    assert_eq!(names, vec!["keep"], "survivor is visible, not deferred");
}
