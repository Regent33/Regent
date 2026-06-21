//! Composition helpers: explicit manifests of which executors back which
//! definitions. Registration is deliberate — never an import side effect.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::TerminalBackend;
use crate::infra::backends::{LocalBackend, terminal_backend_from_env};
use crate::infra::{files, search, terminal, web_search};
use regent_kernel::RegentError;
use std::sync::Arc;

/// The default core toolset (the "narrow waist" — every entry here is paid
/// for on every API call, so additions go up the Footprint Ladder first).
///
/// Uses the host `local` terminal backend; composition roots that serve
/// untrusted input should prefer [`core_catalog_from_env`] so
/// `REGENT_TERMINAL_BACKEND`/`REGENT_SANDBOX` take effect.
#[must_use]
pub fn core_catalog() -> ToolCatalog {
    core_catalog_with_terminal(Arc::new(LocalBackend))
}

/// Core toolset with the terminal backend selected from the environment
/// (`REGENT_TERMINAL_BACKEND`), enforcing `REGENT_SANDBOX` (which forbids the
/// host `local` backend). This is the wiring that makes the docker/ssh/sandbox
/// backends actually reachable.
pub fn core_catalog_from_env() -> Result<ToolCatalog, RegentError> {
    Ok(core_catalog_with_terminal(terminal_backend_from_env()?))
}

/// Core toolset with a chosen terminal backend (docker/ssh sandboxes).
#[must_use]
pub fn core_catalog_with_terminal(backend: Arc<dyn TerminalBackend>) -> ToolCatalog {
    let mut catalog = ToolCatalog::new();
    // Registration of built-ins cannot collide; expect() documents that.
    catalog
        .register(
            terminal::definition(),
            Arc::new(terminal::TerminalTool::with_backend(backend)),
        )
        .expect("core tool 'terminal' registers once");
    catalog
        .register(files::read_definition(), Arc::new(files::ReadFileTool))
        .expect("core tool 'read_file' registers once");
    catalog
        .register(files::write_definition(), Arc::new(files::WriteFileTool))
        .expect("core tool 'write_file' registers once");
    catalog
        .register(search::definition(), Arc::new(search::SearchFilesTool))
        .expect("core tool 'search_files' registers once");
    catalog
        .register(
            web_search::search_definition(),
            Arc::new(web_search::WebSearchTool),
        )
        .expect("core tool 'web_search' registers once");
    catalog
        .register(
            web_search::fetch_definition(),
            Arc::new(web_search::WebFetchTool),
        )
        .expect("core tool 'web_fetch' registers once");
    catalog
}
