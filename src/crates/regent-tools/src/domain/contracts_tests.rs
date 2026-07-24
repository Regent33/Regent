//! Unit tests for `contracts` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

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

/// `DenyAll` is the default live-voice posture: every gated mutation — the
/// unattended shell, desktop control (`computer_use`), app control
/// (`control_app`), file edits, browser actions — is denied, because the caller
/// can't see or veto a prompt. Full control is an explicit opt-in
/// (`REGENT_VOICE_FULL_CONTROL=1` → `AllowAll`), not the default.
#[tokio::test]
async fn deny_all_denies_every_mutation() {
    let approver = DenyAll;
    let voice = VoiceScopedApprover;
    for tool in [
        "terminal",
        "computer_use",
        "control_app",
        "write_file",
        "browser_click",
    ] {
        assert_eq!(
            approver.request(tool, "act", "voice command").await,
            ApprovalDecision::Deny,
            "{tool} must be denied by DenyAll"
        );
        assert_eq!(
            voice.request(tool, "act", "voice command").await,
            ApprovalDecision::Deny,
            "{tool} must be denied under the named voice posture"
        );
    }
}

/// The full-control opt-in (`AllowAll`) still approves the same mutations —
/// deny-by-default must not have broken hands-on voice control.
#[tokio::test]
async fn allow_all_approves_every_mutation_for_full_control() {
    let approver = AllowAll;
    for tool in ["terminal", "computer_use", "control_app", "write_file"] {
        assert_eq!(
            approver.request(tool, "act", "voice command").await,
            ApprovalDecision::Approve,
            "{tool} must be approved under REGENT_VOICE_FULL_CONTROL"
        );
    }
}
