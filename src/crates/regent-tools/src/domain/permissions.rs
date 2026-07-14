//! Permission rules as data (gap S5): last-match-wins wildcard rules over
//! (tool, subject). Split from `contracts.rs` (file-size rule);
//! re-exported there so paths stay stable.

use serde_json::Value;

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
