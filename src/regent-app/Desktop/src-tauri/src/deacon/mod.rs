//! The deacon bridge: spawn the agent backend, pump its notifications to the
//! webview, and expose a request/response client to the command layer. Only
//! this module (and commands.rs) touch Tauri — `rpc` and `spawn` stay pure
//! tokio so they are unit-testable without a running app.

mod rpc;
mod spawn;

#[cfg(test)]
mod tests;

pub use rpc::DeaconRpc;
pub(crate) use spawn::{merged_env, newest_in_target, regent_home};

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
/// shutdown. Behind a Mutex so a later respawn-on-death can swap it in place.
pub struct DeaconState {
    inner: Mutex<Option<DeaconHandle>>,
}

struct DeaconHandle {
    rpc: Arc<DeaconRpc>,
    child: Child,
}

impl DeaconState {
    /// Clone the live client, or `None` when the deacon isn't running.
    pub async fn client(&self) -> Option<Arc<DeaconRpc>> {
        self.inner.lock().await.as_ref().map(|h| Arc::clone(&h.rpc))
    }
}

/// Spawn the deacon and wrap it in managed state. A spawn failure is logged and
/// yields an empty state (the window still opens; commands then report the
/// outage) rather than aborting app startup.
pub async fn spawn_deacon(app: AppHandle) -> DeaconState {
    // Forward every streamed notification to the webview verbatim.
    let emit = move |line: Value| {
        app.emit(DEACON_EVENT, line).ok();
    };
    match spawn::spawn(emit).await {
        Ok((rpc, child)) => DeaconState {
            inner: Mutex::new(Some(DeaconHandle { rpc, child })),
        },
        Err(e) => {
            eprintln!("regent-desktop: deacon unavailable: {e}");
            DeaconState {
                inner: Mutex::new(None),
            }
        }
    }
}

/// Graceful shutdown mirroring spawn.ts: signal stdin EOF, wait up to 2s for a
/// clean drain, then force-kill so exit never hangs on a stuck deacon.
pub async fn shutdown(state: &DeaconState) {
    let mut guard = state.inner.lock().await;
    if let Some(handle) = guard.as_mut() {
        handle.rpc.close_stdin().await;
        if tokio::time::timeout(Duration::from_secs(2), handle.child.wait())
            .await
            .is_err()
        {
            handle.child.kill().await.ok();
        }
    }
}
