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
use regent_providers::ChatProvider;
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
        self.run_turn_with_provider(session_id, text, None).await
    }

    /// Run one turn on an explicitly selected provider/model without changing
    /// the session or app default. The original provider is restored before
    /// releasing the session lock, so concurrent sessions are unaffected.
    pub async fn run_turn_with_model(
        &self,
        session_id: &SessionId,
        text: &str,
        model_override: Option<&str>,
    ) -> Result<String, DeaconError> {
        let provider = model_override.map(|model| self.provider_for_model(model));
        self.run_turn_with_provider(session_id, text, provider)
            .await
    }

    /// Run one turn with an already resolved provider. Explicit routes are
    /// resolved strictly by the dispatcher before this boundary, so they can
    /// never silently fall back to the app default.
    pub async fn run_turn_with_provider(
        &self,
        session_id: &SessionId,
        text: &str,
        provider_override: Option<Arc<dyn ChatProvider>>,
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
        let override_model = provider_override
            .as_ref()
            .map(|provider| provider.model().to_owned());
        let provider_was_overridden = provider_override.is_some();
        // A model/key/config change since this session's provider was built?
        // Swap in a fresh one so the change applies to THIS turn, not just new
        // sessions. Costs the cached prompt prefix — the user asked to switch.
        let epoch = self.routing_epoch();
        if let Some(provider) = provider_override {
            agent.set_provider(provider);
            agent.mark_provider_routed();
        } else if epoch_arc.load(std::sync::atomic::Ordering::Acquire) != epoch {
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
        // W3 step 5 — the memory canary: memory saved since this session's
        // prompt was frozen, which is the only thing retrieval can add that the
        // block does not already carry (see `memory_canary`). Off by default,
        // and a no-op on an ordinary turn. After escalation, which rebuilds
        // the prompt this dedupes against.
        let text = &*self.canary_note(session_id, text).await;

        agent.reset_interrupt();
        let agent_cancel = agent.cancel_handle();

        let session_cancel = CancellationToken::new();
        *interrupt_arc.lock().await = Some(session_cancel.clone());

        let watcher = tokio::spawn(async move {
            session_cancel.cancelled().await;
            agent_cancel.cancel();
        });

        let result = agent.run_turn(text).await;
        // Reveal-on-stuck grows Tier-0 tool definitions inside the turn. Rebase
        // while this agent's turn mutex is still held, before any next turn can
        // clear the attribution or telemetry can inspect the old baseline.
        if agent.last_cache_reset() == Some("tiering") {
            let definitions = serde_json::to_string(&agent.tool_definitions()).unwrap_or_default();
            self.rebase_tool_definitions(session_id, &definitions).await;
        }
        // Emit post-turn context usage so the CLI status line can show the
        // context-fill bar + model (Hermes-style). Best-effort; other surfaces
        // (HTTP/gateway) don't read this notification, so it's harmless there.
        if result.is_ok() {
            let (context_tokens, max_context_tokens) = agent.context_usage();
            let (input_tokens, output_tokens) = agent.last_turn_usage();
            let usage_complete = agent.last_turn_usage_complete();
            let last_request_input_tokens = agent.last_request_input_tokens();
            let model = override_model.clone().unwrap_or_else(|| {
                self.current_model
                    .lock()
                    .map(|m| m.clone())
                    .unwrap_or_default()
            });
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
                    "usage_complete": usage_complete,
                    "last_request_input_tokens": last_request_input_tokens,
                    "context_max": max_context_tokens,
                    // Additive: how much of `context_tokens` is the tool
                    // catalog. It is fixed per turn and not reducible by
                    // compaction, so "73% full" reads very differently when a
                    // large slice of it is schemas the user never sent.
                    "tool_schema_tokens": agent.tool_schema_tokens(),
                    // Additive: the estimate at which compaction fires and the
                    // session splits into a summarized child. 100% of the window
                    // is the wrong landmark for a meter — this is the one that
                    // actually happens to the user. `null` when compaction can't
                    // fire (disabled, or breaker open).
                    "compact_at_tokens": agent.compaction_threshold(),
                    // This is deliberately an estimate of the NEXT request;
                    // provider-reported observed request size is separate.
                    "context_estimated": true,
                    "model": model,
                    // Additive: per-phase wall clock for this turn, in ms.
                    // Until now no surface could say WHY a reply took as long
                    // as it did — only that it had.
                    "timings_ms": super::turn_meta::timings_json(agent.last_turn_timings()),
                }),
            );
            if let Ok(line) = serde_json::to_string(&notification) {
                self.out_tx.send(line).ok();
            }
        }
        if provider_was_overridden {
            agent.set_provider(self.provider());
            epoch_arc.store(epoch, std::sync::atomic::Ordering::Release);
            // The following default-model turn also starts a fresh provider
            // cache because this one-shot turn intentionally changed routes.
            agent.mark_provider_routed();
        }
        watcher.abort();
        *interrupt_arc.lock().await = None;
        // Move the idle clock: this session is demonstrably alive, so the
        // sweep must not treat it as walked-away-from for another interval.
        if let Some(entry) = self.entries.lock().await.get(session_id) {
            entry
                .last_turn_at
                .store(super::now_epoch(), std::sync::atomic::Ordering::Release);
        }
        result.map_err(DeaconError::Core)
    }

    /// Learn from conversations the user has walked away from.
    ///
    /// The background reviewer only fires once a session accumulates
    /// `min_new_messages` (8), and until now the ONLY thing that flushed a
    /// session below that gate was `drain()` on full process shutdown. A
    /// desktop app that stays open for days therefore never learned from short
    /// chats at all — the common case. This sweeps sessions idle for
    /// `IDLE_REVIEW_AFTER_SECS` and flushes their tail.
    ///
    /// Deliberately server-side rather than a client "session ended" signal:
    /// CLI, Desktop, and the platform gateways would each have to send one,
    /// and none of them can say when a conversation is truly over anyway.
    /// Idleness is the honest proxy, and it works for every surface at once.
    ///
    /// Returns how many sessions were flushed (for logging and tests).
    pub async fn sweep_idle_reviews(&self) -> usize {
        let now = super::now_epoch();
        let candidates: Vec<_> = {
            let entries = self.entries.lock().await;
            entries
                .values()
                .filter(|entry| {
                    let last = entry
                        .last_turn_at
                        .load(std::sync::atomic::Ordering::Acquire);
                    now.saturating_sub(last) >= super::IDLE_REVIEW_AFTER_SECS
                })
                .map(|entry| (Arc::clone(&entry.agent), Arc::clone(&entry.last_turn_at)))
                .collect()
        };
        let mut flushed = 0;
        for (agent, stamp) in candidates {
            // try_lock, never lock: a session mid-turn is not idle, whatever
            // its stamp says, and blocking here would stall the sweep behind a
            // multi-minute model call.
            let Ok(mut agent) = agent.try_lock() else {
                continue;
            };
            if let Some(handle) = agent.flush_review() {
                flushed += 1;
                // Re-stamp so a still-open session isn't re-swept every tick;
                // its next real turn will move the clock again anyway.
                stamp.store(now, std::sync::atomic::Ordering::Release);
                tokio::spawn(handle);
            }
        }
        if flushed > 0 {
            tracing::info!(flushed, "idle sessions reviewed");
        }
        flushed
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
