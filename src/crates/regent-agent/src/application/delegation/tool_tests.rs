//! Unit tests for `tool` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use regent_kernel::ToolDefinition;

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
