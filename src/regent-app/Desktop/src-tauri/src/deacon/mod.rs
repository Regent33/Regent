//! The deacon bridge: spawn the agent backend, pump its notifications to the
//! webview, and expose a request/response client to the command layer. Only
//! this module (and commands.rs) touch Tauri — `rpc`, `spawn`, `locate`, and
//! `env` stay pure tokio/std so they are unit-testable without a running app.

mod env;
mod locate;
mod rpc;
mod spawn;

#[cfg(test)]
mod tests;

pub use rpc::DeaconRpc;
pub(crate) use env::{merged_env, regent_home};
pub(crate) use locate::newest_in_target;

use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::process::Child;
use tokio::sync::Mutex;

/// The Tauri event the webview subscribes to for every streamed deacon
/// notification (deltas, tool calls, turn/session lifecycle). The payload is
/// the raw JSON-RPC line, so `session_id` is preserved and the UI can filter
/// by session — a regression guard, do not strip it (CHANGELOG 2026-07-06).
const DEACON_EVENT: &str = "deacon-event";

/// Managed state: the live RPC client plus the child handle for graceful
/// shutdown. Behind a Mutex so respawn-on-death can swap it in place — and so
/// concurrent requests that trigger a respawn are single-flighted.
pub struct DeaconState {
    inner: Mutex<Option<DeaconHandle>>,
}

struct DeaconHandle {
    rpc: Arc<DeaconRpc>,
    child: Child,
}

impl DeaconState {
    /// An unstarted bridge — no deacon yet. Managed synchronously in `setup` so
    /// the first command has state to respawn into, while the actual spawn +
    /// health probe runs OFF the UI thread (see `run`): the event loop, and so
    /// the first window paint, never waits on the deacon's boot. The first
    /// `client_or_respawn` (the background prime, or the webview's first request
    /// — whichever wins the mutex) spawns exactly one deacon.
    #[must_use]
    pub fn unstarted() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Return a live client, respawning the deacon if it never started or has
    /// died. Serialized by the state mutex: a burst of requests arriving after a
    /// crash respawns exactly once and shares the fresh handle (the existing
    /// "Retry" button then actually recovers, instead of re-hitting a dead pipe).
    pub async fn client_or_respawn(&self, app: &AppHandle) -> Result<Arc<DeaconRpc>, String> {
        let app = app.clone();
        self.ensure_client_with(move || spawn::spawn(emitter(app)))
            .await
    }

    /// Core of `client_or_respawn`, with the spawn injected so tests can drive
    /// the respawn/single-flight logic without a Tauri `AppHandle`.
    async fn ensure_client_with<F, Fut>(&self, spawn_fn: F) -> Result<Arc<DeaconRpc>, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(Arc<DeaconRpc>, Child), String>>,
    {
        // The lock is held across the (rare) respawn await on purpose: a second
        // caller waits here, then sees the fresh live handle and skips spawning.
        let mut guard = self.inner.lock().await;
        if let Some(handle) = guard.as_mut() {
            let running = handle.child.try_wait().is_ok_and(|status| status.is_none());
            if running && !handle.rpc.is_dead() {
                return Ok(Arc::clone(&handle.rpc));
            }
        }
        let (rpc, child) = spawn_fn().await?;
        let ret = Arc::clone(&rpc);
        // Replacing the handle drops the old one; kill_on_drop reaps its child.
        *guard = Some(DeaconHandle { rpc, child });
        Ok(ret)
    }
}

/// The notification sink for a spawned deacon: forward every streamed line to
/// the webview verbatim. `Clone` so each probed candidate gets its own copy.
fn emitter(app: AppHandle) -> impl Fn(Value) + Send + Sync + Clone + 'static {
    move |line: Value| {
        app.emit(DEACON_EVENT, line).ok();
    }
}

/// Graceful shutdown mirroring spawn.ts: signal stdin EOF, wait up to 25s for a
/// clean drain, then force-kill so exit never hangs on a stuck deacon.
pub async fn shutdown(state: &DeaconState) {
    let mut guard = state.inner.lock().await;
    if let Some(handle) = guard.as_mut() {
        handle.rpc.close_stdin().await;
        // 25s, not 2s: stdin EOF triggers the deacon's drain, which now
        // FLUSHES the learning loop (session tails reviewed at close, bounded
        // 20s inside the deacon). The deacon exits the moment drain finishes,
        // so this only waits long when reviews are actually in flight —
        // killing at 2s silently discarded that learning.
        if tokio::time::timeout(Duration::from_secs(25), handle.child.wait())
            .await
            .is_err()
        {
            handle.child.kill().await.ok();
        }
    }
}
