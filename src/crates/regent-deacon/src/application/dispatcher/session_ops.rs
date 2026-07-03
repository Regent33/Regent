//! Session/turn lifecycle handlers: create/resume/list/search, the streamed
//! `prompt.submit` turn, interrupt, and approval response.

use super::Dispatcher;
use crate::domain::entities::{RpcNotification, RpcRequest, err_response, ok_response};
use crate::domain::errors::DeaconError;
use regent_kernel::{RegentError, SessionId};
use serde_json::json;
use std::sync::Arc;

impl Dispatcher {
    pub(super) async fn session_create(&self, req: RpcRequest) {
        match self.sessions.create_session().await {
            Ok(id) => self.send(ok_response(req.id, json!({"session_id": id.to_string()}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) async fn session_resume(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        match self
            .sessions
            .resume_session(SessionId::from_string(s))
            .await
        {
            Ok(id) => self.send(ok_response(req.id, json!({"session_id": id.to_string()}))),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn session_list(&self, req: RpcRequest) {
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;
        match self.sessions.list_sessions(limit) {
            Ok(list) => {
                let items: Vec<_> = list
                    .iter()
                    .map(|m| {
                        json!({
                            "session_id": m.id, "source": m.source, "model": m.model,
                            "message_count": m.message_count, "started_at": m.started_at,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    pub(super) fn session_search(&self, req: RpcRequest) {
        let Some(query) = req.params.get("query").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing query"));
            return;
        };
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as u32;
        match self.sessions.search_sessions(query, limit) {
            Ok(hits) => {
                let items: Vec<_> = hits
                    .iter()
                    .map(|h| {
                        json!({
                            "session_id": h.session_id, "role": h.role,
                            "snippet": h.snippet, "timestamp": h.timestamp,
                        })
                    })
                    .collect();
                self.send(ok_response(req.id, json!(items)));
            }
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// Submits a prompt and streams the turn: a `turn.started` notification,
    /// then (from a spawned task) `message.complete`/`turn.complete` (or
    /// `turn.interrupted`) followed by the final JSON-RPC response.
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
        let Some(text) = req
            .params
            .get("text")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
        else {
            self.send(err_response(req.id, -32602, "missing text"));
            return;
        };
        let session_id = SessionId::from_string(sid_str.clone());
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
                    notify(
                        "turn.complete",
                        json!({"session_id": session_id.to_string()}),
                    );
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

    pub(super) async fn turn_interrupt(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let cancelled = self.sessions.interrupt(&SessionId::from_string(s)).await;
        self.send(ok_response(req.id, json!({"cancelled": cancelled})));
    }

    pub(super) async fn approval_respond(&self, req: RpcRequest) {
        let Some(s) = req.params.get("session_id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let approved = req
            .params
            .get("approved")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let resolved = self
            .sessions
            .resolve_approval(&SessionId::from_string(s), approved)
            .await;
        self.send(ok_response(req.id, json!({"resolved": resolved})));
    }
}

/// Turn a raw provider/core turn error into one clear, actionable sentence for
/// the user — shown in chat and spoken on a call. The common, self-inflicted
/// causes (no credit, bad key, rate limit) get a specific fix; anything else
/// passes through as a short, non-JSON summary so the caller still hears why a
/// turn produced nothing instead of dead air.
fn humanize_turn_error(raw: &str) -> String {
    let low = raw.to_lowercase();
    let has = |needle: &str| low.contains(needle);
    if has("402") || has("more credits") || has("insufficient") || has("out of credit") || has("can only afford") {
        return "Your AI provider is out of credits. Add credit to your provider account (for OpenRouter, top up at openrouter.ai) and try again.".into();
    }
    if has("401") || has("unauthorized") || has("invalid api key") || has("no auth credentials") {
        return "Your AI provider rejected the API key. Set a valid model provider key and try again.".into();
    }
    if has("429") || has("rate limit") || has("rate-limit") || has("too many requests") {
        return "Your AI provider is rate-limiting right now. Wait a few seconds and try again.".into();
    }
    if has("404") && has("model") || has("no endpoints found") || has("not a valid model") {
        return "Your configured model isn't available from the provider. Check the model id and try again.".into();
    }
    // Unknown: a trimmed, JSON-free summary so it's still legible when spoken.
    let brief: String = raw.split(&['{', '\n'][..]).next().unwrap_or(raw).trim().chars().take(160).collect();
    format!("I couldn't reach the model. {brief}")
}

#[cfg(test)]
mod tests {
    use super::humanize_turn_error;

    #[test]
    fn credit_and_auth_errors_become_actionable_sentences() {
        let credit = humanize_turn_error(
            "core: provider failure: API error (HTTP 402): {\"error\":{\"message\":\"This request requires more credits, or fewer max_tokens. You requested up to 65536 tokens, but can only afford 31441\"}}",
        );
        assert!(credit.to_lowercase().contains("out of credits"), "{credit}");
        assert!(!credit.contains('{'), "no raw JSON when spoken: {credit}");

        assert!(humanize_turn_error("API error (HTTP 401): unauthorized")
            .to_lowercase()
            .contains("api key"));
        assert!(humanize_turn_error("HTTP 429: rate limit exceeded")
            .to_lowercase()
            .contains("rate-limiting"));
        // Unknown errors keep a short, JSON-free summary.
        let other = humanize_turn_error("core: some weird failure\n{\"detail\":1}");
        assert!(other.starts_with("I couldn't reach the model."), "{other}");
        assert!(!other.contains('{'), "{other}");
    }
}
