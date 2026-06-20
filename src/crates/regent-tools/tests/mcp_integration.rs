//! MCP integration contract: server tools register namespaced into the
//! catalog, dispatch round-trips through the client, failures come back as
//! error JSON, and dispatch hooks observe every call.

use or_mcp::{McpError, McpTool};
use regent_tools::infra::mcp_tools::McpInvoker;
use regent_tools::{DenyAll, DispatchHook, ToolCatalog, ToolContext, register_mcp_tools};
use schemars::Schema;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

fn fake_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "add".into(),
            description: "Adds two numbers.".into(),
            input_schema: Schema::default(),
        },
        McpTool {
            name: "weather".into(),
            description: "Current weather for a city.".into(),
            input_schema: Schema::default(),
        },
    ]
}

/// Stand-in for a connected MCP client (the shape `register_mcp_http`
/// builds from a real `NexusClient`).
fn fake_invoker() -> McpInvoker {
    Arc::new(|name, args| {
        Box::pin(async move {
            if name == "weather" {
                return Err(McpError::Protocol("upstream down".into()));
            }
            Ok(json!({"tool": name, "echo": args}))
        })
    })
}

fn ctx() -> ToolContext {
    ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
}

#[derive(Default)]
struct CountingHook {
    before: AtomicU32,
    after: AtomicU32,
}

impl DispatchHook for CountingHook {
    fn before_dispatch(&self, _tool: &str, _args: &Value) {
        self.before.fetch_add(1, Ordering::SeqCst);
    }

    fn after_dispatch(&self, _tool: &str, _result: &str) {
        self.after.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn mcp_tools_register_namespaced_and_round_trip() {
    let mut catalog = ToolCatalog::new();
    let names = register_mcp_tools(&mut catalog, fake_tools(), fake_invoker(), "calc").unwrap();
    assert_eq!(names, vec!["calc_add", "calc_weather"]);

    let definitions = catalog.definitions();
    let add = definitions.iter().find(|d| d.name == "calc_add").unwrap();
    assert_eq!(add.toolset, "mcp-calc");
    assert_eq!(add.description, "Adds two numbers.");

    // Round-trip: local name maps back to the remote tool name.
    let out = catalog.dispatch("calc_add", r#"{"a": 1, "b": 2}"#, &ctx()).await;
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["tool"], "add");
    assert_eq!(value["echo"]["b"], 2);

    // Upstream failure is data for the model, not a loop crash.
    let out = catalog.dispatch("calc_weather", "{}", &ctx()).await;
    assert!(out.contains("MCP tool failed"));

    // Re-registering the same server collides loudly (no silent shadowing).
    assert!(register_mcp_tools(&mut catalog, fake_tools(), fake_invoker(), "calc").is_err());
}

#[tokio::test]
async fn dispatch_hooks_observe_every_executed_call() {
    let mut catalog = ToolCatalog::new();
    register_mcp_tools(&mut catalog, fake_tools(), fake_invoker(), "calc").unwrap();
    let hook = Arc::new(CountingHook::default());
    catalog.add_hook(hook.clone());

    catalog.dispatch("calc_add", "{}", &ctx()).await;
    catalog.dispatch("calc_weather", "{}", &ctx()).await; // error path still observed
    catalog.dispatch("unknown_tool", "{}", &ctx()).await; // never executed → not observed

    assert_eq!(hook.before.load(Ordering::SeqCst), 2);
    assert_eq!(hook.after.load(Ordering::SeqCst), 2);
}
