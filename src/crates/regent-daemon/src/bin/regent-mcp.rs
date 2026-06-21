//! `regent mcp serve` — exposes Regent's tool catalog as an MCP server over
//! stdio (an external MCP client spawns this process). stdout carries the JSON-
//! RPC stream; logs go to stderr. The catalog is the core tools plus memory and
//! skills (read from `$REGENT_HOME`); tools run with DenyAll approval, so a
//! dangerous shell command is blocked at the guard rather than run for a remote
//! caller.

use regent_skills::{FsSkillRepository, SkillLibrary};
use regent_store::Store;
use regent_tools::{
    DenyAll, StdioServerTransport, ToolContext, core_catalog, register_memory_tools,
    register_skill_tools, serve_catalog, server_card,
};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // stderr only — stdout is the MCP JSON-RPC stream.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    if let Err(error) = run().await {
        // EOF on stdin is the normal shutdown path; report anything else.
        eprintln!("mcp serve stopped: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let home = regent_home();
    let store = Arc::new(Store::open(&home.join("state.db"))?);
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(SkillLibrary::new(Arc::new(FsSkillRepository::new(
        home.join("skills"),
    )?)));

    // Core tools + memory + skills. Session-coupled tools (delegate, send_message,
    // kanban) are deliberately omitted — they belong to a running agent.
    let mut catalog = core_catalog();
    register_memory_tools(&mut catalog, Arc::clone(&graph), Arc::clone(&store))?;
    register_skill_tools(&mut catalog, Arc::clone(&skills))?;

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let ctx = ToolContext::new(cwd, Arc::new(DenyAll));
    tracing::info!(
        tools = catalog.len(),
        "regent mcp serve — exposing catalog over stdio"
    );

    serve_catalog(
        StdioServerTransport::new(),
        Arc::new(catalog),
        ctx,
        server_card(),
    )
    .await?;
    Ok(())
}

fn regent_home() -> PathBuf {
    if let Ok(custom) = std::env::var("REGENT_HOME") {
        return custom.into();
    }
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(base).join(".regent")
}
