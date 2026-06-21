//! Browser control via an external Playwright-compatible MCP server (the same
//! mechanism Claude Code uses). Enable it by pointing `REGENT_BROWSER_MCP_URL`
//! at a running server, e.g.:
//!
//!   npx @playwright/mcp@latest --port 8931
//!   regent keys set REGENT_BROWSER_MCP_URL http://127.0.0.1:8931/sse
//!
//! Read-only actions (navigate / snapshot / screenshot / read) run freely;
//! mutating ones (click / type / fill / submit / key / evaluate / upload) are
//! **approval-gated**. Attachment is best-effort: a missing or unreachable
//! server logs a warning and leaves the catalog unchanged, never breaking a turn.

use crate::application::catalog::ToolCatalog;
use crate::infra::mcp_tools::register_mcp_http_gated;

/// Env var holding the browser MCP server URL (unset = browser control off).
pub const BROWSER_MCP_ENV: &str = "REGENT_BROWSER_MCP_URL";

/// Attach the browser MCP tools to `catalog` if `REGENT_BROWSER_MCP_URL` is set.
pub async fn attach_browser_if_configured(catalog: &mut ToolCatalog) {
    let url = match std::env::var(BROWSER_MCP_ENV) {
        Ok(u) if !u.trim().is_empty() => u,
        _ => return,
    };
    match register_mcp_http_gated(catalog, url.trim(), "browser", needs_approval).await {
        Ok(names) => tracing::info!(count = names.len(), "browser control attached"),
        Err(error) => {
            tracing::warn!(%error, "browser MCP not attached — is the server running at the URL?");
        }
    }
}

/// Mutating browser actions require approval; read/navigate do not.
fn needs_approval(tool: &str) -> bool {
    const GATED: &[&str] = &[
        "click", "type", "fill", "press", "select", "drag", "upload", "evaluate", "submit", "key",
    ];
    let t = tool.to_ascii_lowercase();
    GATED.iter().any(|g| t.contains(g))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_mutating_actions_only() {
        for t in [
            "browser_click",
            "browser_type",
            "browser_fill_form",
            "browser_press_key",
            "browser_evaluate",
        ] {
            assert!(needs_approval(t), "{t} should be gated");
        }
        for t in [
            "browser_navigate",
            "browser_snapshot",
            "browser_take_screenshot",
            "browser_wait_for",
        ] {
            assert!(!needs_approval(t), "{t} should be free");
        }
    }
}
