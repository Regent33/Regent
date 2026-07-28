//! Unit tests for `tool` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use regent_kernel::ToolDefinition;
use tokio_util::sync::CancellationToken;

struct NoProvider; // `child_catalog` never calls the model
#[async_trait]
impl ChatProvider for NoProvider {
    async fn complete(
        &self,
        _request: &regent_providers::ChatRequest,
    ) -> Result<regent_providers::ChatResponse, regent_providers::ProviderError> {
        unreachable!("child_catalog does not run the agent")
    }
    fn model(&self) -> &str {
        "none"
    }
}

struct Noop;
#[async_trait]
impl ToolExecutor for Noop {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        Ok("{}".into())
    }
}

fn tool_at(depth: usize, max_depth: usize) -> DelegateTool {
    let mut leaf = ToolCatalog::new();
    let def = ToolDefinition {
        name: "search".into(),
        description: "leaf tool".into(),
        parameters: json!({"type": "object"}),
        toolset: "leaf".into(),
    };
    leaf.register(def, Arc::new(Noop)).unwrap();
    DelegateTool {
        provider: Arc::new(NoProvider),
        store: Arc::new(Store::open_in_memory().unwrap()),
        leaf_catalog: Arc::new(leaf),
        config: DelegationConfig {
            max_depth,
            ..DelegationConfig::default()
        },
        depth,
    }
}

fn has(catalog: &ToolCatalog, name: &str) -> bool {
    catalog.definitions().iter().any(|d| d.name == name)
}
#[test]
fn below_cap_children_can_delegate_one_more_level() {
    let child = tool_at(1, 2).child_catalog();
    assert!(has(&child, "search"), "leaf tools are preserved");
    assert!(has(&child, "delegate_task"), "below cap → child may nest");
}

#[test]
fn at_cap_children_get_leaf_only() {
    let grandchild = tool_at(2, 2).child_catalog();
    assert!(has(&grandchild, "search"), "leaf tools still present");
    assert!(
        !has(&grandchild, "delegate_task"),
        "at cap → recursion stops"
    );
}

#[test]
fn max_depth_one_reproduces_leaf_only_behavior() {
    assert!(!has(&tool_at(1, 1).child_catalog(), "delegate_task"));
}

struct DenyAll;
#[async_trait]
impl regent_tools::ApprovalHandler for DenyAll {
    async fn request(
        &self,
        _tool: &str,
        _subject: &str,
        _why: &str,
    ) -> regent_tools::ApprovalDecision {
        regent_tools::ApprovalDecision::Deny
    }
}

/// `interrupt_subagent`: pressing stop must reach the children, not just the
/// parent's loop.
///
/// Delegation is synchronous and each child is a whole agent with its own
/// 50-iteration budget, so before this the user's stop was observed only after
/// the entire fan-out finished — minutes of model calls already paid for.
/// `Agent::new` now ADOPTS a token already on the context instead of minting a
/// fresh one, which links parent to child without the delegation code knowing
/// tokens exist.
///
/// `NoProvider::complete` is `unreachable!`, so this test fails loudly — by
/// panic, not by assertion — if a child reaches the model after the stop.
#[tokio::test]
async fn a_cancelled_parent_stops_its_children_before_they_call_the_model() {
    let tool = tool_at(1, 2);
    let cancel = CancellationToken::new();
    cancel.cancel(); // the user pressed stop
    let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll)).with_cancel(cancel);

    let out = tool
        .execute(json!({"tasks": ["one", "two"]}), &ctx)
        .await
        .unwrap();
    let parsed: Value = serde_json::from_str(&out).unwrap();
    let results = parsed["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "order-preserving fan-out is unchanged");
    for result in results {
        assert_eq!(
            result["status"], "failed",
            "a child of a stopped parent must not run: {result}"
        );
    }
}
