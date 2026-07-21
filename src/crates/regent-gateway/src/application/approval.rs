//! Approval-over-chat: a dangerous tool action sends a prompt to the chat
//! and blocks until `/approve` or `/deny` arrives (routed by the runner) —
//! or times out. Non-response is a **deny**, never proceed-by-default
//! (a core invariant).

use crate::domain::contracts::PlatformAdapter;
use crate::domain::entities::OutboundMessage;
use async_trait::async_trait;
use regent_tools::{ApprovalDecision, ApprovalHandler};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// How long one approval of a tool covers further use of the SAME tool without
/// re-prompting — so "type X in the search bar" (focus field, then type: two
/// computer_use actions) is a SINGLE approval, not one per keystroke-step.
/// Sliding: each action within the window extends it. `REGENT_APPROVAL_GRACE_SECS=0`
/// disables coalescing (prompt on every action). Scoped per tool AND per chat.
const DEFAULT_GRACE_SECS: u64 = 120;

fn approval_grace() -> Duration {
    let secs = std::env::var("REGENT_APPROVAL_GRACE_SECS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_GRACE_SECS);
    Duration::from_secs(secs)
}

/// Whether `tool` was approved recently enough (within `grace`) to skip
/// re-prompting. A different tool, an expired entry, or a zero grace all mean
/// "ask again".
fn within_grace(recent: &HashMap<String, Instant>, tool: &str, grace: Duration) -> bool {
    !grace.is_zero() && recent.get(tool).is_some_and(|at| at.elapsed() < grace)
}

/// Routes chat replies to whichever approval is pending in that chat.
#[derive(Default)]
pub struct ApprovalRouter {
    pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

impl ApprovalRouter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a pending approval for `chat_key`. A previous pending one
    /// in the same chat is dropped (its waiter resolves to deny).
    pub fn register(&self, chat_key: &str) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .expect("approval mutex poisoned")
            .insert(chat_key.to_owned(), sender);
        receiver
    }

    /// `/approve` / `/deny` arrived. Returns false when nothing was pending.
    pub fn resolve(&self, chat_key: &str, approved: bool) -> bool {
        match self
            .pending
            .lock()
            .expect("approval mutex poisoned")
            .remove(chat_key)
        {
            Some(sender) => sender.send(approved).is_ok(),
            None => false,
        }
    }
}

/// `regent_tools::ApprovalHandler` bound to one chat: the gateway's answer
/// to the CLI's stdin y/N prompt.
pub struct ChatApprovalHandler {
    adapter: Arc<dyn PlatformAdapter>,
    router: Arc<ApprovalRouter>,
    chat_key: String,
    chat_id: String,
    timeout: Duration,
    grace: Duration,
    /// Per-tool last-approval time (this chat only) for grace coalescing.
    recent: Mutex<HashMap<String, Instant>>,
}

impl ChatApprovalHandler {
    #[must_use]
    pub fn new(
        adapter: Arc<dyn PlatformAdapter>,
        router: Arc<ApprovalRouter>,
        chat_key: impl Into<String>,
        chat_id: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            adapter,
            router,
            chat_key: chat_key.into(),
            chat_id: chat_id.into(),
            timeout,
            grace: approval_grace(),
            recent: Mutex::new(HashMap::new()),
        }
    }

    /// Record an approval so the next same-tool action within the grace window
    /// skips the prompt.
    fn remember(&self, tool: &str) {
        self.recent
            .lock()
            .expect("approval grace mutex poisoned")
            .insert(tool.to_owned(), Instant::now());
    }

    /// Clear the grace so the next dangerous action prompts again. Called at the
    /// start of every turn: coalescing is scoped to ONE user request (its
    /// multi-step sequence), never carried into the next message.
    pub fn reset_grace(&self) {
        self.recent
            .lock()
            .expect("approval grace mutex poisoned")
            .clear();
    }
}

/// Auto mode (`tools.auto_approve` in config.yaml, or `REGENT_AUTO_APPROVE=1`).
///
/// Read from disk **per request** rather than cached at startup, deliberately:
/// the gateway is a long-lived background process, and a user who switches auto
/// mode OFF in the app must have that take effect without remembering to
/// restart it. One small file read per approval gate is nothing next to the
/// model call that triggered it.
fn auto_approve_enabled() -> bool {
    if std::env::var("REGENT_AUTO_APPROVE").is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes"))
    {
        return true;
    }
    // Chat-toggled gateway flag (`/auto on`) — a gateway can be auto without the
    // desktop being, since it's a different trust context (your own bot).
    if gateway_auto_on() {
        return true;
    }
    // Also honour the shared `tools.auto_approve` (set from the app / CLI).
    let Ok(raw) = std::fs::read_to_string(regent_home().join("config.yaml")) else {
        return false;
    };
    serde_yaml::from_str::<serde_yaml::Value>(&raw)
        .ok()
        .and_then(|cfg| cfg.get("tools")?.get("auto_approve")?.as_bool())
        .unwrap_or(false)
}

/// `$REGENT_HOME` else `~/.regent` — the resolution every surface uses.
fn regent_home() -> std::path::PathBuf {
    std::env::var("REGENT_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let base = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            std::path::PathBuf::from(base).join(".regent")
        })
}

/// Presence of the `gateway-auto` flag file = auto mode on for this gateway.
#[must_use]
pub fn gateway_auto_on() -> bool {
    regent_home().join("gateway-auto").exists()
}

/// Toggle the gateway auto-mode flag (chat `/auto on|off`). Takes effect on the
/// very next approval — the flag is read per request, not cached.
pub fn set_gateway_auto(on: bool) -> std::io::Result<()> {
    let path = regent_home().join("gateway-auto");
    if on {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, b"1")
    } else {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[async_trait]
impl ApprovalHandler for ChatApprovalHandler {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision {
        if auto_approve_enabled() {
            tracing::info!(tool, reason, "auto-approved (tools.auto_approve)");
            return ApprovalDecision::Approve;
        }
        // Grace: a multi-step sequence of the same tool (focus a field, then
        // type into it) is one task → one approval, not one prompt per step.
        {
            let recent = self.recent.lock().expect("approval grace mutex poisoned");
            if within_grace(&recent, tool, self.grace) {
                drop(recent);
                self.remember(tool); // sliding window
                tracing::info!(tool, "auto-approved within grace of a recent approval");
                return ApprovalDecision::Approve;
            }
        }
        let receiver = self.router.register(&self.chat_key);
        let prompt = OutboundMessage {
            chat_id: self.chat_id.clone(),
            text: format!(
                "⚠ {tool} wants to run a dangerous action ({reason}):\n{action}\n\nReply /approve or /deny — denying in {}s otherwise.",
                self.timeout.as_secs()
            ),
        };
        if let Err(error) = self.adapter.send(prompt).await {
            tracing::warn!(%error, "could not deliver approval prompt; denying");
            return ApprovalDecision::Deny;
        }
        match tokio::time::timeout(self.timeout, receiver).await {
            Ok(Ok(true)) => {
                self.remember(tool); // start the grace window for follow-up steps
                ApprovalDecision::Approve
            }
            // timeout, dropped sender, or explicit deny — all deny.
            _ => ApprovalDecision::Deny,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Auto mode is read per request, not cached: switching it OFF in the app
    /// has to reach a gateway that has been running for days. The env var is
    /// process-global, so this test owns the config-file path only.
    #[test]
    fn auto_approve_follows_the_config_file_between_calls() {
        let dir = tempfile::tempdir().expect("tempdir");
        // SAFETY: single-threaded test; no other thread reads REGENT_HOME here.
        unsafe {
            std::env::set_var("REGENT_HOME", dir.path());
            std::env::remove_var("REGENT_AUTO_APPROVE");
        }

        // No config at all → never auto-approve.
        assert!(!auto_approve_enabled(), "must fail closed without config");

        let config = dir.path().join("config.yaml");
        std::fs::write(&config, "tools:\n  auto_approve: true\n").unwrap();
        assert!(auto_approve_enabled(), "on when the file says on");

        std::fs::write(&config, "tools:\n  auto_approve: false\n").unwrap();
        assert!(!auto_approve_enabled(), "OFF must apply without a restart");

        unsafe { std::env::remove_var("REGENT_HOME") };
    }

    #[test]
    fn grace_coalesces_the_same_tool_only() {
        let mut recent = HashMap::new();
        recent.insert("computer_use".to_owned(), Instant::now());
        let grace = Duration::from_secs(120);

        // The just-approved tool is graced; a different tool still prompts.
        assert!(within_grace(&recent, "computer_use", grace));
        assert!(!within_grace(&recent, "terminal", grace));
        // Grace of 0 disables coalescing entirely (prompt every action).
        assert!(!within_grace(&recent, "computer_use", Duration::ZERO));

        // An approval older than the window re-prompts.
        let mut stale = HashMap::new();
        stale.insert(
            "computer_use".to_owned(),
            Instant::now() - Duration::from_secs(300),
        );
        assert!(!within_grace(&stale, "computer_use", grace));
    }
}
