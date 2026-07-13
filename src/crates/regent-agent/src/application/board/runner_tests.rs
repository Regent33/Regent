//! Unit tests for `runner` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use regent_kernel::ToolDefinition;
use regent_providers::{ChatRequest, ChatResponse, ProviderError};
use serde_json::json;

struct NoProvider;
#[async_trait]
impl ChatProvider for NoProvider {
    async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        unreachable!("resolve() never calls the model")
    }
    fn model(&self) -> &str {
        "none"
    }
}

/// A provider whose `model()` is a fixed tag — lets a test assert *which*
/// provider `resolve()` picked without calling the model.
struct Tagged(&'static str);
#[async_trait]
impl ChatProvider for Tagged {
    async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        unreachable!("resolve() never calls the model")
    }
    fn model(&self) -> &str {
        self.0
    }
}

struct Noop;
#[async_trait]
impl regent_tools::ToolExecutor for Noop {
    async fn execute(
        &self,
        _a: serde_json::Value,
        _c: &ToolContext,
    ) -> Result<String, RegentError> {
        Ok("{}".into())
    }
}

fn def(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: "t".into(),
        parameters: json!({"type": "object"}),
        toolset: "core".into(),
    }
}

fn task(assignee: Option<&str>) -> KanbanTaskRow {
    KanbanTaskRow {
        id: "t1".into(),
        board: "default".into(),
        title: "do it".into(),
        description: String::new(),
        status: "in_progress".into(),
        assignee: assignee.map(ToOwned::to_owned),
        created_at: 0.0,
        updated_at: 0.0,
    }
}

fn runner(store: Arc<Store>) -> AgentTaskRunner {
    let mut catalog = ToolCatalog::new();
    catalog.register(def("search"), Arc::new(Noop)).unwrap();
    catalog.register(def("write_file"), Arc::new(Noop)).unwrap();
    AgentTaskRunner::new(
        Arc::new(NoProvider),
        Arc::new(catalog),
        store,
        ToolContext::new(std::env::temp_dir(), Arc::new(regent_tools::DenyAll)),
        "default worker prompt",
    )
}

fn names(c: &ToolCatalog) -> Vec<String> {
    c.definitions().into_iter().map(|d| d.name).collect()
}

#[test]
fn unassigned_task_uses_default_prompt_and_full_catalog() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let r = runner(Arc::clone(&store));
    let (prompt, cat, _provider) = r.resolve(&task(None));
    assert_eq!(prompt, "default worker prompt");
    assert_eq!(names(&cat).len(), 2);
}

#[test]
fn named_agent_overrides_prompt_and_restricts_tools() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    store
        .upsert_agent("researcher", "web", "You research.", None, Some("search"))
        .unwrap();
    let r = runner(Arc::clone(&store));
    let (prompt, cat, _provider) = r.resolve(&task(Some("researcher")));
    assert_eq!(prompt, "You research.");
    assert_eq!(
        names(&cat),
        vec!["search".to_owned()],
        "tool allow-list applied"
    );
}

#[test]
fn named_agent_with_model_runs_on_the_resolved_provider() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    store
        .upsert_agent("fast", "quick", "Be quick.", Some("groq/llama"), None)
        .unwrap();
    // Resolver only knows the configured model; anything else ⇒ default.
    let resolver: ProviderResolver = Arc::new(|m: &str| {
        (m == "groq/llama").then(|| Arc::new(Tagged("groq-llama")) as Arc<dyn ChatProvider>)
    });
    let r = runner(Arc::clone(&store)).with_resolver(resolver);
    let (_prompt, _cat, provider) = r.resolve(&task(Some("fast")));
    assert_eq!(
        provider.model(),
        "groq-llama",
        "ran on its resolved provider"
    );
}

#[test]
fn named_agent_model_without_resolver_falls_back_to_default_provider() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    store
        .upsert_agent("fast", "quick", "Be quick.", Some("groq/llama"), None)
        .unwrap();
    let r = runner(Arc::clone(&store)); // no resolver attached
    let (_prompt, _cat, provider) = r.resolve(&task(Some("fast")));
    assert_eq!(
        provider.model(),
        "none",
        "default provider used (NoProvider)"
    );
}

#[test]
fn unknown_assignee_falls_back_to_default() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let r = runner(Arc::clone(&store));
    let (prompt, cat, _provider) = r.resolve(&task(Some("regent")));
    assert_eq!(prompt, "default worker prompt");
    assert_eq!(names(&cat).len(), 2);
}
