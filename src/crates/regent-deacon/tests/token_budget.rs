//! First-turn input-token budget measurement (MEASUREMENT, not behavior).
//!
//! Prints a per-component token estimate (chars/4, consistent) of a fresh
//! session's first-turn input: the prompt layers (system prompt, capabilities,
//! constitution core), plus EVERY main-catalog tool schema serialized the way
//! the Anthropic provider sends it (`{name, description, input_schema}`),
//! sorted by cost. It then reports the model-facing total with NO deferral vs.
//! the shipped `ToolsConfig::default().deferred` list, so the deferral lever's
//! savings are visible and repeatable.
//!
//! Run it with:
//!   cargo test -p regent-deacon --test token_budget -- --ignored --nocapture

use async_trait::async_trait;
use regent_agent::{AgentConfig, CAPABILITIES, SYSTEM_PROMPT, constitution_core};
use regent_deacon::SessionManager;
use regent_kernel::ToolDefinition;
use regent_providers::{ChatProvider, ChatRequest, ChatResponse, ProviderError};
use regent_skills::{FsSkillRepository, SkillLibrary};
use regent_store::Store;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc::unbounded_channel;

struct NullProvider;

#[async_trait]
impl ChatProvider for NullProvider {
    async fn complete(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Err(ProviderError::Parse("not used".into()))
    }
    fn model(&self) -> &str {
        "measure"
    }
}

/// chars/4 — the whole file uses this one estimator so numbers are comparable.
fn toks(s: &str) -> usize {
    s.chars().count().div_ceil(4)
}

/// A tool schema as the Anthropic provider serializes it on the wire.
fn tool_wire_json(def: &ToolDefinition) -> String {
    json!({
        "name": def.name,
        "description": def.description,
        "input_schema": def.parameters,
    })
    .to_string()
}

/// Faithfully sizes the synthesized `load_tools` schema for a given deferred
/// set — its description carries an 80-char hook per deferred tool (mirrors
/// `ToolCatalog::defer`), so more deferral makes this one tool bigger.
fn load_tools_tokens(defs: &[ToolDefinition], deferred: &[String]) -> usize {
    let index: String = deferred
        .iter()
        .filter_map(|n| defs.iter().find(|d| &d.name == n))
        .map(|d| {
            let hook: String = d.description.chars().take(60).collect();
            format!("{} ({hook}…)", d.name)
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let def = json!({
        "name": "load_tools",
        "description": format!(
            "Load the full schema of deferred tools, making them callable. More tools \
             exist than are listed — load one when its purpose matches the task: {index}"
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "names": {"type": "array", "items": {"type": "string"},
                          "description": "Deferred tool names to load."}
            },
            "required": ["names"]
        }
    });
    toks(&def.to_string())
}

#[tokio::test]
#[ignore = "measurement, run with --ignored --nocapture"]
async fn first_turn_token_budget() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(SkillLibrary::new(Arc::new(
        FsSkillRepository::new(dir.path().join("skills")).unwrap(),
    )));
    let (tx, _rx) = unbounded_channel();
    let provider: Arc<dyn ChatProvider> = Arc::new(NullProvider);
    let factory: regent_deacon::ProviderFactory = Arc::new(move |_m| Arc::clone(&provider));
    let sm = Arc::new(SessionManager::new(
        factory,
        "measure",
        store,
        graph,
        skills,
        PathBuf::from("."),
        AgentConfig::default(),
        regent_deacon::ToolsConfig::default(),
        tx,
    ));
    // Installs the self-handle so the in-process `regent`, `code_task`, and
    // `background_task` tools register — a real session always has them.
    sm.install_admin(regent_deacon::AdminDeps::default());

    let defs = sm.list_tool_definitions().await.unwrap();

    // ── Prompt layers (env-independent, fresh-session shape) ──────────────
    // now/artifacts/voice lines and the persona/memory/skills blocks are empty
    // in a fresh text session (no REGENT_* env, empty DB, no seeded skills).
    let constitution = constitution_core("Regent");
    let sys = toks(SYSTEM_PROMPT);
    let caps = toks(CAPABILITIES);
    let con = toks(&constitution);

    println!("\n=== FIRST-TURN INPUT TOKEN BUDGET (chars/4) ===\n");
    println!("PROMPT LAYERS");
    println!("  {:>6}  system_prompt (SYSTEM_PROMPT)", sys);
    println!("  {:>6}  capabilities (CAPABILITIES)", caps);
    println!("  {:>6}  constitution (always-on core, seeded persona row)", con);
    println!(
        "  {:>6}  persona/soul + user profile  (empty in a fresh session)",
        0
    );
    println!("  {:>6}  memory block  (empty in a fresh session)", 0);
    println!("  {:>6}  skills index  (data-dependent; empty test library)", 0);
    let prompt_total = sys + caps + con;
    println!("  ------  ");
    println!("  {:>6}  PROMPT SUBTOTAL", prompt_total);

    // ── Tool schemas, sorted by cost ─────────────────────────────────────
    let default_deferred = regent_deacon::ToolsConfig::default().deferred;
    let mut rows: Vec<(String, usize, bool)> = defs
        .iter()
        .map(|d| {
            (
                d.name.clone(),
                toks(&tool_wire_json(d)),
                default_deferred.contains(&d.name),
            )
        })
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));

    println!("\nTOOL SCHEMAS  ({} registered)  [D]=deferred by default", defs.len());
    let mut all_tools = 0;
    let mut deferred_saved = 0;
    for (name, t, deferred) in &rows {
        all_tools += *t;
        if *deferred {
            deferred_saved += *t;
        }
        println!(
            "  {:>6}  {} {}",
            t,
            if *deferred { "[D]" } else { "   " },
            name
        );
    }

    let load_tools = load_tools_tokens(&defs, &default_deferred);
    println!("\nTOTALS");
    println!("  {:>6}  all tool schemas (no deferral)", all_tools);
    println!("  {:>6}  withheld by default deferral", deferred_saved);
    println!("  {:>6}  load_tools schema (added by deferral)", load_tools);
    let tools_model_facing = all_tools - deferred_saved + load_tools;
    println!("  {:>6}  model-facing tool schemas (with default deferral)", tools_model_facing);

    println!("\nFIRST-TURN INPUT");
    println!(
        "  {:>6}  NO deferral    = prompt {} + all tools {}",
        prompt_total + all_tools,
        prompt_total,
        all_tools
    );
    println!(
        "  {:>6}  WITH default   = prompt {} + model-facing tools {}",
        prompt_total + tools_model_facing,
        prompt_total,
        tools_model_facing
    );
    println!();
}
