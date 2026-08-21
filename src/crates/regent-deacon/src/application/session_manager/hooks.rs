//! Per-session plumbing: the RPC approval handler, the tool-event hook, and
//! the session-registry entry. Constructed by the manager's lifecycle code.

use crate::domain::contracts::OutboundTx;
use crate::domain::entities::RpcNotification;
use async_trait::async_trait;
use regent_agent::Agent;
use regent_kernel::RegentError;
use regent_kernel::contracts::questionnaire::{Questionnaire, QuestionnaireAnswer, render_text};
use regent_tools::{
    ApprovalDecision, ApprovalHandler, DeliverySink, DispatchHook, text_decision_to_answer,
};
use serde_json::json;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

/// What resolves a parked request. One slot serves both because only one thing
/// can be in front of the human at a time — a questionnaire carries ALL its
/// questions, so "1 of 3" is a stepper inside one request, not three of them.
#[derive(Debug)]
pub(super) enum ApprovalReply {
    /// `approval.respond` — the verdict plus optional free text: deny-feedback
    /// for a tool gate (gap S6), or the answer to a plain `ask_user` (gap T4).
    Verdict {
        approved: bool,
        feedback: Option<String>,
    },
    /// `question.respond` — a typed answer to a structured questionnaire.
    Question(Box<QuestionnaireAnswer>),
}

pub(super) type ApprovalTx = oneshot::Sender<ApprovalReply>;

/// Bridges a tool approval request to the client over JSON-RPC: emits
/// `approval.request`, then blocks on a oneshot resolved by `approval.respond`
/// (deny on timeout — never proceed without an explicit yes).
pub(super) struct RpcApprovalHandler {
    /// Filled after `Agent::new` returns (the agent generates the id).
    pub(super) session_id: Arc<OnceLock<String>>,
    pub(super) out_tx: OutboundTx,
    pub(super) pending: Arc<Mutex<Option<ApprovalTx>>>,
    /// The connected client declared `capabilities: ["questions"]` on
    /// `session.create`. False → a structured question is rendered as numbered
    /// text down the existing `approval.request` path, so a shipped CLI or app
    /// keeps working unchanged against a new deacon.
    pub(super) supports_questions: Arc<AtomicBool>,
}

impl RpcApprovalHandler {
    /// Park a oneshot in the session's single pending slot and emit `notif`.
    async fn park_and_emit(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> oneshot::Receiver<ApprovalReply> {
        let (tx, rx) = oneshot::channel();
        *self.pending.lock().await = Some(tx);
        if let Ok(line) = serde_json::to_string(&RpcNotification::new(method, params)) {
            self.out_tx.send(line).ok();
        }
        rx
    }

    /// Whether the connected client declared it can render a question card.
    pub(super) fn supports_questions(&self) -> bool {
        self.supports_questions.load(Ordering::Acquire)
    }

    fn sid(&self) -> String {
        self.session_id.get().cloned().unwrap_or_default()
    }
}

#[async_trait]
impl ApprovalHandler for RpcApprovalHandler {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision {
        let sid = self.sid();
        let rx = self
            .park_and_emit(
                "approval.request",
                json!({ "session_id": sid, "tool": tool, "action": action, "reason": reason }),
            )
            .await;

        match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
            Ok(Ok(ApprovalReply::Verdict { approved: true, .. })) => ApprovalDecision::Approve,
            // A denial carrying text steers the model (gap S6) — and doubles
            // as the free-text answer channel for `ask_user`.
            Ok(Ok(ApprovalReply::Verdict {
                feedback: Some(feedback),
                ..
            })) if !feedback.trim().is_empty() => ApprovalDecision::DenyWithFeedback(feedback),
            Ok(Ok(ApprovalReply::Verdict { .. })) => ApprovalDecision::Deny,
            // A card answer arriving at a plain gate: any answer at all means
            // the human acted, so treat it as the text they gave rather than
            // stalling the turn to a timeout.
            Ok(Ok(ApprovalReply::Question(answer))) => {
                if answer.cancelled {
                    ApprovalDecision::Deny
                } else {
                    ApprovalDecision::Approve
                }
            }
            _ => {
                *self.pending.lock().await = None;
                ApprovalDecision::Deny
            }
        }
    }

    async fn request_structured(&self, questionnaire: &Questionnaire) -> QuestionnaireAnswer {
        // An old client never learned `question.request`; sending it one would
        // stall the turn for the full timeout with nothing on screen.
        if !self.supports_questions.load(Ordering::Acquire) {
            let decision = self
                .request("ask_user", &render_text(questionnaire), "")
                .await;
            return text_decision_to_answer(questionnaire, &decision);
        }

        let sid = self.sid();
        let rx = self
            .park_and_emit(
                "question.request",
                json!({ "session_id": sid, "questionnaire": questionnaire }),
            )
            .await;

        let cancelled = || QuestionnaireAnswer {
            questionnaire_id: questionnaire.id.clone(),
            answers: Vec::new(),
            cancelled: true,
        };
        match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
            // A stale card answering a newer question would put the wrong
            // words in the user's mouth — drop it and let the turn proceed
            // on "no answer".
            Ok(Ok(ApprovalReply::Question(answer)))
                if answer.questionnaire_id == questionnaire.id =>
            {
                *answer
            }
            // A client that only knows approvals still resolves the turn.
            Ok(Ok(ApprovalReply::Verdict { approved, feedback })) => text_decision_to_answer(
                questionnaire,
                &match (approved, feedback) {
                    (true, _) => ApprovalDecision::Approve,
                    (false, Some(text)) if !text.trim().is_empty() => {
                        ApprovalDecision::DenyWithFeedback(text)
                    }
                    (false, _) => ApprovalDecision::Deny,
                },
            ),
            Ok(Ok(ApprovalReply::Question(_))) => cancelled(),
            _ => {
                *self.pending.lock().await = None;
                cancelled()
            }
        }
    }
}

/// Emits `tool.start` / `tool.complete` around every dispatch (the ADR-011
/// event surface; the CLI renders these as activity lines).
pub(super) struct RpcToolHook {
    pub(super) session_id: Arc<OnceLock<String>>,
    pub(super) out_tx: OutboundTx,
}

impl RpcToolHook {
    fn emit(&self, method: &str, params: serde_json::Value) {
        if let Ok(line) = serde_json::to_string(&RpcNotification::new(method, params)) {
            self.out_tx.send(line).ok();
        }
    }
}

/// Cap for the args/result disclosure fields — enough to identify the call in
/// an activity row without streaming a whole tool payload to the client.
const SUMMARY_MAX_CHARS: usize = 500;
const CODE_DETAIL_MAX_CHARS: usize = 20_000;

/// Truncate on a char boundary, appending an ellipsis when clipped.
fn summarize(text: &str) -> String {
    if text.chars().count() > SUMMARY_MAX_CHARS {
        let mut s: String = text.chars().take(SUMMARY_MAX_CHARS).collect();
        s.push('…');
        s
    } else {
        text.to_owned()
    }
}

fn bounded_code(text: &str) -> String {
    if text.chars().count() > CODE_DETAIL_MAX_CHARS {
        let mut clipped: String = text.chars().take(CODE_DETAIL_MAX_CHARS).collect();
        clipped.push_str("\n… clipped …");
        clipped
    } else {
        text.to_owned()
    }
}

/// Safe, bounded disclosure for code-writing tools only. Arbitrary tool
/// arguments are deliberately excluded: terminal commands and service calls
/// can contain credentials or unrelated private payloads.
pub(crate) fn code_detail(tool: &str, args: &serde_json::Value) -> Option<serde_json::Value> {
    let string = |key: &str| args.get(key).and_then(serde_json::Value::as_str);
    match tool {
        "file_edit" => Some(json!({
            "kind": "replace",
            "path": string("path"),
            "before": bounded_code(string("old_string")?),
            "after": bounded_code(string("new_string")?),
        })),
        "write_file" => Some(json!({
            "kind": "write",
            "path": string("path"),
            "after": bounded_code(string("content")?),
        })),
        "apply_patch" => Some(json!({
            "kind": "patch",
            "patch": bounded_code(string("patch")?),
        })),
        _ => None,
    }
}

impl DispatchHook for RpcToolHook {
    fn before_dispatch(&self, tool: &str, args: &serde_json::Value) {
        let sid = self.session_id.get().cloned().unwrap_or_default();
        // `args_summary` (additive): the serialized tool input, truncated — so
        // the CLI can disclose *what* a tool was called with, not just its name.
        let args_summary = summarize(&args.to_string());
        self.emit(
            "tool.start",
            json!({
                "session_id": sid,
                "tool": tool,
                "args_summary": args_summary,
                "args_detail": code_detail(tool, args),
            }),
        );
    }

    fn after_dispatch(&self, tool: &str, result: &str) {
        let sid = self.session_id.get().cloned().unwrap_or_default();
        let is_error = serde_json::from_str::<serde_json::Value>(result)
            .is_ok_and(|v| v.get("error").is_some());
        // `result_summary` + `ok` (additive): a truncated result preview and a
        // plain success flag alongside the existing `is_error`.
        self.emit(
            "tool.complete",
            json!({
                "session_id": sid, "tool": tool, "is_error": is_error,
                "ok": !is_error, "result_summary": summarize(result),
            }),
        );
    }
}

/// ADR-038 P2: the tools whose call escalates a light session to full. THE
/// single source of truth for the trigger set — the P0 measurement proxy
/// (`Store::session_mix`'s LIKE patterns in regent-store `session_mix.rs`)
/// mirrors this list by hand; change BOTH or the live escalation rate and
/// the analytic `escalation_share` silently measure different events.
pub(super) const ESCALATION_TRIGGERS: &[&str] = &["load_tools", "code_task", "delegate_task"];

/// ADR-038 P2: flips `pending` when the model reaches for an agentic tool
/// from a LIGHT session. `run_turn` applies the escalation on the NEXT turn
/// (never mid-turn — the prompt is frozen per request). Attached only to
/// light catalogs, so full sessions pay nothing.
pub(super) struct EscalationHook {
    pub(super) pending: Arc<std::sync::atomic::AtomicBool>,
}

impl DispatchHook for EscalationHook {
    fn before_dispatch(&self, tool: &str, _args: &serde_json::Value) {
        if ESCALATION_TRIGGERS.contains(&tool) {
            self.pending
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }

    fn after_dispatch(&self, _tool: &str, _result: &str) {}
}

/// One live session in the registry.
pub(super) struct SessionEntry {
    pub(super) agent: Arc<Mutex<Agent>>,
    /// Cancel the currently running turn; None when no turn is active.
    pub(super) interrupt: Arc<Mutex<Option<CancellationToken>>>,
    /// Oneshot sender to resolve a pending `approval.request`.
    pub(super) approval_pending: Arc<Mutex<Option<ApprovalTx>>>,
    /// Routing epoch this session's provider was built at; `run_turn` swaps in
    /// a fresh provider when the manager's epoch has moved past it.
    pub(super) provider_epoch: Arc<std::sync::atomic::AtomicU64>,
    /// Build-time stable-prefix baseline (SPL §3.1). `turn_prefix_hashes`
    /// checks each turn's actual prompt + re-serialized tool definitions
    /// against it — fail-open, warn-only. Replaced (once) on P2 escalation —
    /// the full profile is a new baseline, not a bust of the old one.
    pub(super) ledger: Arc<crate::domain::ledger::Ledger>,
    /// ADR-038: `true` while this session runs the `light` profile; flips to
    /// `false` exactly once, when `run_turn` applies an escalation (one-way).
    pub(super) light: Arc<std::sync::atomic::AtomicBool>,
    /// Set by [`EscalationHook`] mid-turn; consumed by `run_turn` next turn.
    pub(super) escalate_pending: Arc<std::sync::atomic::AtomicBool>,
    /// The conversation key the session was built with — an escalation rebuild
    /// must reproduce the same catalog surface (platform tools ride the key).
    pub(super) conversation_key: Option<String>,
    /// Epoch seconds of this session's last finished turn, for the idle-review
    /// sweep. A conversation the user walked away from is the natural moment to
    /// learn from it — before this, only full process shutdown ever flushed the
    /// review, so a session under the batch gate was never learned from at all.
    pub(super) last_turn_at: Arc<std::sync::atomic::AtomicU64>,
    /// The project folder this session opened (Desktop "Open Folder"); `None`
    /// means it runs in the manager's own cwd, which is every CLI, platform,
    /// and background session. Fixed at creation — switching trees means a new
    /// session, so an escalation rebuild must reproduce this too.
    pub(super) workspace: Option<PathBuf>,
    /// W3 step 5: node ids the memory canary already injected here. The frozen
    /// block never gains a post-freeze entry, so without this the same memory
    /// is re-prepended on every later turn that recalls it [co-audit].
    pub(super) canary_seen: Arc<std::sync::Mutex<HashSet<String>>>,
}

/// Deacon-native delivery sink for `send_message`: the connected surface *is*
/// the channel, so a delivery becomes a `message.outbound` notification the CLI
/// renders. (Real platform sinks — Telegram/Discord — plug in at the gateway.)
pub(super) struct NotificationDelivery {
    pub(super) session_id: Arc<OnceLock<String>>,
    pub(super) out_tx: OutboundTx,
}

#[async_trait]
impl DeliverySink for NotificationDelivery {
    async fn deliver(&self, target: &str, text: &str) -> Result<(), RegentError> {
        let sid = self.session_id.get().cloned().unwrap_or_default();
        let to = if target.is_empty() { "home" } else { target };
        let notif = RpcNotification::new(
            "message.outbound",
            json!({ "session_id": sid, "target": to, "text": text }),
        );
        if let Ok(line) = serde_json::to_string(&notif) {
            self.out_tx.send(line).ok();
        }
        Ok(())
    }

    fn targets(&self) -> Vec<String> {
        vec!["home".to_owned()]
    }
}

#[cfg(test)]
#[path = "tests/hooks.rs"]
mod tests;
