//! `regent-deacon mcp` — exposes Regent's tool catalog as an MCP server over
//! stdio (an external MCP client spawns this process). stdout carries the JSON-
//! RPC stream; logs go to stderr. The catalog is the core tools plus memory and
//! skills (read from `$REGENT_HOME`); tools run with DenyAll approval, so a
//! dangerous shell command is blocked at the guard rather than run for a remote
//! caller.
//!
//! This was its own `regent-mcp` binary until it turned out never to have been
//! packaged by CI or either installer — so `regent mcp serve` failed with
//! "regent-mcp not found" on every machine that installed rather than built
//! from source. It is a subcommand of the deacon instead of a fourth 49MB
//! executable because it already lives in this crate: same dependencies, same
//! catalog wiring, so folding it in costs no archive bytes at all.

use crate::boot::regent_home;
use regent_skills::{FsSkillRepository, SkillLibrary};
use regent_store::Store;
use regent_tools::{
    DenyAll, StdioServerTransport, ToolContext, core_catalog_from_env, register_memory_tools,
    register_skill_tools, serve_catalog, server_card,
};
use std::sync::Arc;

/// Serve the catalog over stdio, then exit — this never returns to the caller,
/// so none of the daemon wiring in `main::run` is reached.
pub(crate) async fn serve() -> ! {
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
    std::process::exit(0);
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let home = regent_home()?;
    let store = Arc::new(Store::open(&home.join("state.db"))?);
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(SkillLibrary::new(Arc::new(FsSkillRepository::new(
        home.join("skills"),
    )?)));

    // Core tools + memory + skills. Session-coupled tools (delegate, send_message,
    // kanban) are deliberately omitted — they belong to a running agent.
    // `core_catalog_from_env` so REGENT_SANDBOX / REGENT_TERMINAL_BACKEND are
    // honoured here too. `core_catalog()` skips that enforcement entirely, so
    // an MCP server started with the sandbox flag set ran host commands anyway.
    let mut catalog = core_catalog_from_env()?;
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
