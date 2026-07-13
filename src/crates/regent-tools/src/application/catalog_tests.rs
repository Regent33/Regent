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
