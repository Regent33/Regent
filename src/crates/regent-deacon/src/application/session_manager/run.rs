//! The operational methods on `SessionManager`: `run_turn` (one turn on a
//! session — live credential re-merge, routing-epoch provider swap, one-way
//! profile escalation, then the interruptible turn) and `drain` (cancel
//! in-flight turns and flush the learning loop at shutdown). Split from the
//! registry struct in `mod.rs` for size; the struct's private fields are
//! reachable here because this module is a descendant of the one defining it.

use super::SessionManager;
use crate::domain::entities::RpcNotification;
use crate::domain::errors::DeaconError;
use regent_kernel::SessionId;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

impl SessionManager {
    pub async fn run_turn(
        &self,
        session_id: &SessionId,
        text: &str,
    ) -> Result<String, DeaconError> {
        // Pick up keys saved since this deacon started — the Settings panel,
        // manage_keys, and hand edits all write $REGENT_HOME/.env. A long-lived
        // deacon (notably the voice server's, which survives app restarts) would
        // otherwise never see a new key: re-merge credential vars so it works
        // THIS turn, no restart (the "keys/vision don't update live" report).
        let merged = regent_tools::reload_credentials_from_dotenv();
        if merged > 0 {
            tracing::info!(merged, "re-merged updated credentials from .env");
        }
        let (agent_arc, interrupt_arc, epoch_arc, light_arc, escalate_arc, conversation_key) = {
            let entries = self.entries.lock().await;
            match entries.get(session_id) {
                Some(e) => (
                    Arc::clone(&e.agent),
                    Arc::clone(&e.interrupt),
                    Arc::clone(&e.provider_epoch),
                    Arc::clone(&e.light),
                    Arc::clone(&e.escalate_pending),
                    e.conversation_key.clone(),
                ),
                None => return Err(DeaconError::SessionNotFound(session_id.to_string())),
            }
        };

        let mut agent = agent_arc.lock().await;
        // A model/key/config change since this session's provider was built?
        // Swap in a fresh one so the change applies to THIS turn, not just new
        // sessions. Costs the cached prompt prefix — the user asked to switch.
        let epoch = self.routing_epoch();
        if epoch_arc.load(std::sync::atomic::Ordering::Acquire) != epoch {
            agent.set_provider(self.provider());
            epoch_arc.store(epoch, std::sync::atomic::Ordering::Release);
            // SPL P2 (§3.1): a routing swap warms the new provider's cache cold —
            // stamp this turn so `turn.complete` attributes the full-price turn.
            agent.mark_provider_routed();
        }
        // ADR-038 P2: a light session whose last turn reached for an agentic
        // tool escalates NOW, before this turn's model call — rebuild the
        // prompt+catalog as full, one-way (never a downgrade; oscillation
        // busts caches). Fail-open: a rebuild error leaves the session light
        // and the flag set, so the next turn retries.
        if light_arc.load(std::sync::atomic::Ordering::Acquire)
            && escalate_arc.load(std::sync::atomic::Ordering::Acquire)
        {
            match self
                .escalate_to_full(session_id, &mut agent, conversation_key.as_deref())
                .await
            {
                Ok(()) => {
                    light_arc.store(false, std::sync::atomic::Ordering::Release);
                    if let Err(e) = self.store.mark_session_escalated(session_id) {
                        tracing::warn!(session = %session_id, error = %e, "escalation stamp failed");
                    }
                }
                Err(e) => {
                    tracing::warn!(session = %session_id, error = %e,
                                   "profile escalation failed; staying light this turn");
                }
            }
        }
        agent.reset_interrupt();
        let agent_cancel = agent.cancel_handle();

        let session_cancel = CancellationToken::new();
        *interrupt_arc.lock().await = Some(session_cancel.clone());

        let watcher = tokio::spawn(async move {
            session_cancel.cancelled().await;
            agent_cancel.cancel();
        });

        let result = agent.run_turn(text).await;
        // Emit post-turn context usage so the CLI status line can show the
        // context-fill bar + model (Hermes-style). Best-effort; other surfaces
        // (HTTP/gateway) don't read this notification, so it's harmless there.
        if result.is_ok() {
            let (context_tokens, max_context_tokens) = agent.context_usage();
            let (input_tokens, output_tokens) = agent.last_turn_usage();
            let model = self
                .current_model
                .lock()
                .map(|m| m.clone())
                .unwrap_or_default();
            let notification = RpcNotification::new(
                "turn.usage",
                json!({
                    "session_id": session_id.to_string(),
                    "context_tokens": context_tokens,
                    "max_context_tokens": max_context_tokens,
                    // Additive (M8 status-bar context meter): the just-finished
                    // turn's token spend + the context budget under the name the
                    // desktop expects. `context_max` == `max_context_tokens`.
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "context_max": max_context_tokens,
                    "model": model,
                }),
            );
            if let Ok(line) = serde_json::to_string(&notification) {
                self.out_tx.send(line).ok();
            }
        }
        watcher.abort();
        *interrupt_arc.lock().await = None;
        result.map_err(DeaconError::Core)
    }

    /// Cancels every in-flight turn, then waits briefly so cancelled turns
    /// finish recording their ledger rows before the process exits.
    pub async fn drain(&self) {
        let (interrupts, agents): (Vec<_>, Vec<_>) = {
            let entries = self.entries.lock().await;
            (
                entries.values().map(|e| Arc::clone(&e.interrupt)).collect(),
                entries.values().map(|e| Arc::clone(&e.agent)).collect(),
            )
        };
        let mut cancelled_any = false;
        for arc in interrupts {
            if let Some(token) = arc.lock().await.as_ref() {
                token.cancel();
                cancelled_any = true;
            }
        }
        if cancelled_any {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        // Learning-loop flush: sessions closed under the batch gate would
        // otherwise never be reviewed — their tail is learned from NOW.
        // try_lock skips an agent still mid-cancellation (its partial turn is
        // low-signal anyway); the await is bounded so shutdown never hangs on
        // a slow review model.
        let mut flushes = Vec::new();
        for agent in agents {
            if let Ok(mut agent) = agent.try_lock()
                && let Some(handle) = agent.flush_review()
            {
                flushes.push(handle);
            }
        }
        if !flushes.is_empty() {
            let count = flushes.len();
            let all = async {
                for handle in flushes {
                    let _ = handle.await;
                }
            };
            match tokio::time::timeout(Duration::from_secs(20), all).await {
                Ok(()) => tracing::info!(count, "session-end review flush complete"),
                Err(_) => tracing::warn!(count, "review flush timed out — partial learning saved"),
            }
        }
    }
}
