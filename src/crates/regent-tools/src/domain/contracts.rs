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

/// What a matched permission rule does with the call (gap S5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    Allow,
    /// Route through the surface's [`ApprovalHandler`].
    Ask,
    Deny,
}

/// One permission rule, data not code: `permission` names the tool (`*` = any),
/// `pattern` is a `*`-wildcard match against the call's subject (path, command,
/// URL — falling back to the raw args). Evaluation is last-match-wins, so later
/// rules override earlier ones (allowlist base, targeted overrides on top).
#[derive(Debug, Clone)]
pub struct PermissionRule {
    pub permission: String,
    pub pattern: String,
    pub action: PermissionAction,
    /// Returned to the model when `action` denies (gap S6).
    pub feedback: Option<String>,
}

/// The last rule matching `(tool, subject)`, or `None` (→ default behavior,
/// exactly as if no rules existed).
#[must_use]
pub fn evaluate_permissions<'a>(
    rules: &'a [PermissionRule],
    tool: &str,
    subject: &str,
) -> Option<&'a PermissionRule> {
    rules.iter().rev().find(|rule| {
        (rule.permission == "*" || rule.permission == tool)
            && wildcard_match(&rule.pattern, subject)
    })
}

/// The call's subject for permission matching: the most specific meaningful
/// argument (path / command / url / query), falling back to the raw args.
#[must_use]
pub fn subject_of(args: &Value) -> String {
    for key in ["path", "command", "url", "query"] {
        if let Some(value) = args.get(key).and_then(Value::as_str) {
            return value.to_owned();
        }
    }
    args.to_string()
}

/// `*`-wildcard match ('*' spans any run of characters, everything else is
/// literal). ponytail: backtracking scan, O(n·m) worst case — rules and
/// subjects are short strings; a compiled matcher if rule sets ever grow.
#[must_use]
pub fn wildcard_match(pattern: &str, subject: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == subject,
        Some((head, tail)) => {
            let Some(rest) = subject.strip_prefix(head) else {
                return false;
            };
            (0..=rest.len())
                .filter(|i| rest.is_char_boundary(*i))
                .any(|i| wildcard_match(tail, &rest[i..]))
        }
    }
}

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
mod tests {
    use super::*;

    #[test]
    fn wildcard_matches_literals_stars_and_unicode_boundaries() {
        assert!(wildcard_match("*", "anything at all"));
        assert!(wildcard_match("rm *", "rm -rf /"));
        assert!(!wildcard_match("rm *", "cargo test"));
        assert!(wildcard_match("*/.env", "config/.env"));
        assert!(wildcard_match("*secret*", "my-secret-file.txt"));
        assert!(!wildcard_match("exact", "exact-not"));
        assert!(wildcard_match("exact", "exact"));
        assert!(wildcard_match("*é*", "café au lait"));
    }

    #[test]
    fn permission_rules_evaluate_last_match_wins() {
        let rules = vec![
            PermissionRule {
                permission: "terminal".into(),
                pattern: "*".into(),
                action: PermissionAction::Ask,
                feedback: None,
            },
            PermissionRule {
                permission: "terminal".into(),
                pattern: "cargo *".into(),
                action: PermissionAction::Allow,
                feedback: None,
            },
            PermissionRule {
                permission: "*".into(),
                pattern: "*.env*".into(),
                action: PermissionAction::Deny,
                feedback: Some("secrets stay sealed — ask the user to share what you need".into()),
            },
        ];
        // Later rules override earlier ones.
        let hit = evaluate_permissions(&rules, "terminal", "cargo test").unwrap();
        assert_eq!(hit.action, PermissionAction::Allow);
        let hit = evaluate_permissions(&rules, "terminal", "rm -rf /").unwrap();
        assert_eq!(hit.action, PermissionAction::Ask);
        let hit = evaluate_permissions(&rules, "read_file", "config/.env").unwrap();
        assert_eq!(hit.action, PermissionAction::Deny);
        assert!(hit.feedback.as_deref().unwrap().contains("sealed"));
        // No match → None → default behavior.
        assert!(evaluate_permissions(&rules, "read_file", "src/main.rs").is_none());
    }

    #[test]
    fn denied_is_fail_closed_and_feedback_surfaces() {
        assert!(!ApprovalDecision::Approve.denied());
        assert!(ApprovalDecision::Deny.denied());
        let d = ApprovalDecision::DenyWithFeedback("use apply_patch instead".into());
        assert!(d.denied());
        assert_eq!(d.feedback(), Some("use apply_patch instead"));
        assert_eq!(ApprovalDecision::Deny.feedback(), None);
    }

    /// The voice auto-approver denies only the unattended shell; the GUI
    /// control a caller drives by voice (computer_use / control_app / browser /
    /// file edits) runs on spoken consent (P0-002: computer-use on calls).
    #[tokio::test]
    async fn voice_scoped_approver_denies_only_terminal() {
        let approver = VoiceScopedApprover;
        assert_eq!(
            approver.request("terminal", "rm -rf /", "dangerous").await,
            ApprovalDecision::Deny,
            "the unattended shell must stay denied under voice auto-approve"
        );
        for tool in ["computer_use", "control_app", "write_file", "browser_click"] {
            assert_eq!(
                approver
                    .request(tool, "close the active tab", "voice command")
                    .await,
                ApprovalDecision::Approve,
                "{tool} must run on spoken consent during a call"
            );
        }
    }
}
