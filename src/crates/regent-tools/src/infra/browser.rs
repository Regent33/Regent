//! Browser control via an external Playwright-compatible MCP server (the same
//! mechanism Claude Code uses). Enable it by pointing `REGENT_BROWSER_MCP_URL`
//! at a running server, e.g.:
//!
//!   npx @playwright/mcp@latest --port 8931
//!   printf %s http://127.0.0.1:8931/sse | regent keys set REGENT_BROWSER_MCP_URL --stdin
//!
//! Read-only actions (navigate / snapshot / screenshot / read) run freely;
//! mutating ones (click / type / fill / submit / key / evaluate / upload) are
//! **approval-gated**. Attachment is best-effort: a missing or unreachable
//! server logs a warning and leaves the catalog unchanged, never breaking a turn.

use crate::application::catalog::ToolCatalog;
use crate::infra::mcp_tools::register_mcp_http_gated;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Env var holding the browser MCP server URL (unset = browser control off).
pub const BROWSER_MCP_ENV: &str = "REGENT_BROWSER_MCP_URL";

/// How long a session build waits for the MCP handshake before giving up.
/// This runs on the blocking path of EVERY `session.create`/`session.resume`,
/// and the handshake has no timeout of its own: the transport's `Client` is
/// built with reqwest's default (none), and when the inbox is empty
/// `receive_message` GETs the endpoint and reads the body to completion — an
/// SSE endpoint's body never completes, so a *running* server on a `/sse` URL
/// hangs forever. Attachment is best-effort by contract, so cap it.
const ATTACH_BUDGET: Duration = Duration::from_secs(3);

/// How long to leave the browser alone after a failed attach. Without this the
/// cost is paid again on every chat opened — a server that is simply not
/// running measured ~2.4s per session, which read to the user as the whole app
/// being slow to open folders and past sessions.
const RETRY_AFTER: Duration = Duration::from_secs(60);

/// When the next attach may run (`None` = now). Process-wide: the server is
/// per-machine, not per-session.
static NEXT_ATTEMPT: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

/// Whether an attach may run now. Pure so the backoff is testable without a
/// server, a clock, or a catalog.
fn may_attempt(next_attempt: Option<Instant>, now: Instant) -> bool {
    next_attempt.is_none_or(|at| now >= at)
}

/// Attach the browser MCP tools to `catalog` if `REGENT_BROWSER_MCP_URL` is set.
pub async fn attach_browser_if_configured(catalog: &mut ToolCatalog) {
    let url = match std::env::var(BROWSER_MCP_ENV) {
        Ok(u) if !u.trim().is_empty() => u,
        _ => return,
    };
    // Read-then-drop: never hold a std mutex across an await.
    let gate = *NEXT_ATTEMPT.lock().unwrap_or_else(|e| e.into_inner());
    if !may_attempt(gate, Instant::now()) {
        return;
    }
    let attach = register_mcp_http_gated(catalog, url.trim(), "browser", needs_approval);
    let outcome = match tokio::time::timeout(ATTACH_BUDGET, attach).await {
        Ok(Ok(names)) => {
            tracing::info!(count = names.len(), "browser control attached");
            None
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, "browser MCP not attached — is the server running at the URL?");
            Some(Instant::now() + RETRY_AFTER)
        }
        Err(_) => {
            tracing::warn!(
                url = url.trim(),
                seconds = ATTACH_BUDGET.as_secs(),
                "browser MCP handshake timed out — continuing without browser control"
            );
            Some(Instant::now() + RETRY_AFTER)
        }
    };
    *NEXT_ATTEMPT.lock().unwrap_or_else(|e| e.into_inner()) = outcome;
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

    #[test]
    fn backoff_skips_retries_until_the_window_passes() {
        let now = Instant::now();
        // Never tried, or the window elapsed → attach.
        assert!(may_attempt(None, now));
        assert!(may_attempt(Some(now - Duration::from_secs(1)), now));
        // Inside the window after a failure → skip, so opening a chat stays
        // instant while the server is down.
        assert!(!may_attempt(Some(now + RETRY_AFTER), now));
    }

    #[tokio::test]
    async fn a_dead_server_cannot_delay_a_session_beyond_the_budget() {
        // Port 1 has nothing on it; the point is the CALL returns promptly and
        // leaves the catalog untouched rather than blocking session birth.
        unsafe { std::env::set_var(BROWSER_MCP_ENV, "http://127.0.0.1:1/sse") };
        *NEXT_ATTEMPT.lock().unwrap() = None;
        let mut catalog = ToolCatalog::new();
        let started = Instant::now();
        attach_browser_if_configured(&mut catalog).await;
        unsafe { std::env::remove_var(BROWSER_MCP_ENV) };
        assert!(
            started.elapsed() < ATTACH_BUDGET + Duration::from_secs(2),
            "attach took {:?}",
            started.elapsed()
        );
        assert!(catalog.names().is_empty());
    }
}
