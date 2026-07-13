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
