use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::RegentError;
use serde_json::Value;

/// The executor side of the two-file tool contract (the definition side is
/// `regent_kernel::ToolDefinition`). Executes with parsed arguments;
/// returns a JSON string on success. Errors are wrapped into
/// `{"error": ...}` by the catalog — they never reach the agent loop as
/// exceptions.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approve,
    Deny,
    /// Gap S6: a denial that tells the model WHY and what to do instead —
    /// the text becomes the tool result, so the model steers instead of
    /// stalling on a bare "denied".
    DenyWithFeedback(String),
}

impl ApprovalDecision {
    /// Fail-closed: anything that is not an explicit `Approve` is a denial —
    /// new variants can never slip through an equality check as approval.
    #[must_use]
    pub fn denied(&self) -> bool {
        !matches!(self, Self::Approve)
    }

    /// The denial feedback, when the surface provided one.
    #[must_use]
    pub fn feedback(&self) -> Option<&str> {
        match self {
            Self::DenyWithFeedback(text) => Some(text),
            _ => None,
        }
    }
}

pub use super::permissions::{
    PermissionAction, PermissionRule, evaluate_permissions, subject_of, wildcard_match,
};

/// Human approval gate for dangerous actions. The surface (CLI prompt,
/// gateway message) implements this; executors only ever see the decision.
/// Non-response is the caller's concern and must resolve to `Deny`
/// (never proceed by default).
#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    async fn request(&self, tool: &str, action: &str, reason: &str) -> ApprovalDecision;
}

/// Fail-safe default: everything dangerous is denied.
pub struct DenyAll;

#[async_trait]
impl ApprovalHandler for DenyAll {
    async fn request(&self, _tool: &str, _action: &str, _reason: &str) -> ApprovalDecision {
        ApprovalDecision::Deny
    }
}

/// Approves everything. ONLY for a surface where the human is already directly
/// driving each action and there is no way to prompt (e.g. a live voice call the
/// user is speaking to). Never the default — opt-in per surface.
pub struct AllowAll;

#[async_trait]
impl ApprovalHandler for AllowAll {
    async fn request(&self, _tool: &str, _action: &str, _reason: &str) -> ApprovalDecision {
        ApprovalDecision::Approve
    }
}

/// Auto-approver for live voice calls: the spoken command IS the consent, so
/// GUI control the caller drives by voice — desktop clicks/keys (`computer_use`),
/// app control (`control_app`), file edits, browser actions — runs unattended.
/// Only `terminal` stays denied: an unattended shell is where a misheard word
/// turns into `rm -rf`, and it's irreversible and invisible, unlike a click the
/// caller is watching on screen. `REGENT_VOICE_FULL_CONTROL=1` lifts even that.
/// Screen capture / vision are non-mutating and never reach this gate.
pub struct VoiceScopedApprover;

#[async_trait]
impl ApprovalHandler for VoiceScopedApprover {
    async fn request(&self, tool: &str, _action: &str, _reason: &str) -> ApprovalDecision {
        match tool {
            "terminal" => ApprovalDecision::Deny,
            _ => ApprovalDecision::Approve,
        }
    }
}

/// Where the agent proactively delivers messages — a platform + channel (the
/// gateway's home channel, a Discord/Slack target, …). The surface implements
/// this; the `send_message` tool only names a target. Delivery is an
/// outward-facing action, so the tool layer gates it like any other.
#[async_trait]
pub trait DeliverySink: Send + Sync {
    /// Delivers `text` to `target` (empty → the default home channel).
    async fn deliver(&self, target: &str, text: &str) -> Result<(), RegentError>;

    /// Available delivery targets — surfaced to the model in the tool schema.
    fn targets(&self) -> Vec<String>;

    /// Uploads a local file to `target` with an optional caption. Defaults to
    /// declining, so only surfaces that wire an upload path expose `send_file`.
    async fn deliver_file(
        &self,
        _target: &str,
        _path: &std::path::Path,
        _caption: &str,
    ) -> Result<(), RegentError> {
        Err(RegentError::Tool {
            tool: "send_file".into(),
            message: "file delivery is not available here".into(),
        })
    }
}

/// Fail-safe default: no channels configured, so delivery always declines.
pub struct NoDelivery;

#[async_trait]
impl DeliverySink for NoDelivery {
    async fn deliver(&self, _target: &str, _text: &str) -> Result<(), RegentError> {
        Err(RegentError::Tool {
            tool: "send_message".into(),
            message: "no delivery channels are configured".into(),
        })
    }
    fn targets(&self) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Where terminal commands execute (the environments contract):
/// local shell, a docker container, a remote host over ssh, … The terminal
/// tool owns guard/approval/truncation; backends only run commands.
#[async_trait]
pub trait TerminalBackend: Send + Sync {
    /// Human-readable target, for logs and the tool result.
    fn describe(&self) -> String;

    async fn run(
        &self,
        command: &str,
        cwd: &std::path::Path,
        timeout: std::time::Duration,
    ) -> Result<CommandOutput, RegentError>;
}

/// Observer hooks around every tool dispatch (tracer / audit surface —
/// the in-process plugin seam). Hooks observe; they cannot mutate.
pub trait DispatchHook: Send + Sync {
    fn before_dispatch(&self, tool: &str, args: &Value);
    fn after_dispatch(&self, tool: &str, result: &str);
}

#[cfg(test)]
#[path = "contracts_tests.rs"]
mod tests;
