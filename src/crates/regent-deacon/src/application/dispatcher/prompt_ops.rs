//! The streamed `prompt.submit` turn: delta notifications, queueing, the
//! post-turn telemetry, and the raw-error → actionable-sentence mapping.
//! Split from `session_ops.rs` (file-size rule).

use super::Dispatcher;
use crate::application::session_manager::SessionManager;
use crate::domain::errors::DeaconError;
use crate::domain::entities::{RpcNotification, RpcRequest, err_response, ok_response};
use regent_kernel::{RegentError, SessionId};
use serde_json::json;
use std::sync::Arc;

impl Dispatcher {
    pub(super) fn prompt_submit(&self, req: RpcRequest) {
        let id = req.id.clone();
        let Some(sid_str) = req
            .params
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
        else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let Some(mut text) = req
            .params
            .get("text")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
        else {
            self.send(err_response(req.id, -32602, "missing text"));
            return;
        };
        // The raw opening message drives first-turn title generation — captured
        // before we decorate the prompt with attachment refs / job wrapping.
        let title_source = text.clone();
        let session_id = SessionId::from_string(sid_str.clone());

        // Optional staged attachments (M8): append one ref line per path so the
        // agent's file tools can open it. Only paths under
        // `$REGENT_HOME/attachments` are honored — anything else is rejected so a
        // client can't smuggle an arbitrary filesystem path into the prompt.
        if let Some(items) = req.params.get("attachments").and_then(|v| v.as_array()) {
            let root = super::attachment_ops::attachments_root();
            for item in items {
                let Some(p) = item.as_str() else { continue };
                if !super::attachment_ops::attachment_within_root(&root, std::path::Path::new(p)) {
                    self.send(err_response(
                        req.id,
                        -32602,
                        format!("attachment path is outside the attachments root: {p}"),
                    ));
                    return;
                }
                text.push_str(&format!("\n\n[attached file: {p}]"));
            }
        }

        // Decide up-front whether this turn should title the session: only an
        // untitled session whose first user turn is about to run (checked before
        // `run_turn` appends the user message). Cheap store reads.
        let should_title = SessionManager::should_generate_title(
            self.sessions.session_has_title(&session_id),
            self.sessions.prior_user_turns(&session_id),
        );

        // Deliver background-task results/status with the user's turn — only
        // real client turns pass through here, never detached job sessions.
        let text = crate::application::background_task_tool::wrap_prompt(&text);
        self.notify("turn.started", json!({"session_id": sid_str}));

        let sessions = Arc::clone(&self.sessions);
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let send = |payload: String| {
                out_tx.send(payload).ok();
            };
            let notify = |method: &str, params: serde_json::Value| {
                if let Ok(line) = serde_json::to_string(&RpcNotification::new(method, params)) {
                    out_tx.send(line).ok();
                }
            };
            match sessions.run_turn(&session_id, &text).await {
                Ok(reply) => {
                    notify(
                        "message.complete",
                        json!({"session_id": session_id.to_string(), "reply": reply}),
                    );
                    // Additive (desktop status-bar ctx meter): the just-finished
                    // turn's token spend + context budget. The desktop populates
                    // the meter only when ALL THREE are present, so attach them
                    // to the SUCCESS turn.complete. Best-effort: an unknown
                    // session simply omits them (payload stays back-compatible).
                    let mut complete = json!({"session_id": session_id.to_string()});
                    if let Some((input_tokens, output_tokens, context_max, cache_read, cache_write)) =
                        sessions.last_turn_usage(&session_id).await
                        && let Some(obj) = complete.as_object_mut()
                    {
                        obj.insert("input_tokens".into(), json!(input_tokens));
                        obj.insert("output_tokens".into(), json!(output_tokens));
                        obj.insert("context_max".into(), json!(context_max));
                        // Additive (SPL §3.3): the cached/fresh split, present
                        // only when the provider reported prompt-cache usage.
                        if let Some(read) = cache_read {
                            obj.insert("cache_read_tokens".into(), json!(read));
                        }
                        if let Some(write) = cache_write {
                            obj.insert("cache_write_tokens".into(), json!(write));
                        }
                    }
                    // Additive (SPL §3.1): why this turn was full-price, when
                    // known — omitted entirely when the prefix carried over.
                    if let Some(reason) = sessions.last_turn_cache_reset(&session_id).await
                        && let Some(obj) = complete.as_object_mut()
                    {
                        obj.insert("cache_reset".into(), json!(reason));
                    }
                    // Additive (SPL §3.3): build-time stable-prefix tier hashes
                    // so clients can watch Tier 0/1 stability across turns. The
                    // call also runs the fail-open cache_bust check. Best-effort
                    // like the usage fields — omitted for an unknown session.
                    if let Some((tier0_hash, tier1_hash)) =
                        sessions.turn_prefix_hashes(&session_id).await
                        && let Some(obj) = complete.as_object_mut()
                    {
                        obj.insert("tier0_hash".into(), json!(tier0_hash));
                        obj.insert("tier1_hash".into(), json!(tier1_hash));
                    }
                    notify("turn.complete", complete);
                    // First-turn title generation (M8): a cheap aux model call
                    // names the session, then emits `session.titled` so the rail
                    // updates live. Detached so it never delays the reply, and
                    // best-effort so a failure only warns. Titled from the whole
                    // opening EXCHANGE: call sessions open with a bare "hey
                    // boss" — only the reply carries the topic.
                    if should_title {
                        let sessions = Arc::clone(&sessions);
                        let sid = session_id.clone();
                        let source = crate::application::session_manager::exchange_snippet(
                            &title_source,
                            &reply,
                        );
                        tokio::spawn(async move {
                            sessions.generate_title(sid, source).await;
                        });
                    }
                    let resp = ok_response(
                        id,
                        json!({"reply": reply, "session_id": session_id.to_string()}),
                    );
                    if let Ok(line) = serde_json::to_string(&resp) {
                        send(line);
                    }
                }
                Err(error) => {
                    let interrupted = matches!(&error, DeaconError::Core(RegentError::Interrupted));
                    // Interruptions are internal control flow; every other turn
                    // failure is shown/spoken to the user, so make it a clear,
                    // actionable sentence instead of a raw provider dump.
                    let message = if interrupted {
                        error.to_string()
                    } else {
                        humanize_turn_error(&error.to_string())
                    };
                    notify(
                        if interrupted {
                            "turn.interrupted"
                        } else {
                            "turn.complete"
                        },
                        json!({"session_id": session_id.to_string(), "error": message}),
                    );
                    let resp = err_response(id, -32000, message);
                    if let Ok(line) = serde_json::to_string(&resp) {
                        send(line);
                    }
                }
            }
        });
    }
}

use turn_errors::humanize_turn_error;

mod turn_errors;
