//! Unit tests for `azure_devops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn basic(user: &str, pass: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{user}:{pass}")))
}

#[test]
fn configured_mode_accepts_matching_basic_and_rejects_others() {
    let adapter =
        AzureDevOpsAdapter::new(Some("hook".into()), Some("pw".into()), "pat", "https://x");
    assert!(adapter.verify(b"{}", Some(&basic("hook", "pw")), None));

    // Wrong creds and a missing header fail closed.
    assert!(!adapter.verify(b"{}", Some(&basic("hook", "nope")), None));
    assert!(!adapter.verify(b"{}", Some("Bearer xyz"), None));
    assert!(!adapter.verify(b"{}", None, None));
}

#[test]
fn unconfigured_mode_accepts_anything() {
    let adapter = AzureDevOpsAdapter::new(None, None, "pat", "https://x");
    assert!(adapter.verify(b"{}", None, None));
    assert!(adapter.verify(b"{}", Some("anything"), None));
}

#[test]
fn parses_workitem_event_and_skips_unrelated() {
    let adapter = AzureDevOpsAdapter::new(None, None, "pat", "https://x");

    // Uses the rendered message text and numeric resource id.
    let wi = br#"{"eventType":"workitem.updated",
        "message":{"text":"Bug 42 was updated by Sam"},
        "resource":{"id":42}}"#;
    let events = adapter.parse_webhook(wi).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "42");
    assert_eq!(
        events[0].text,
        "[workitem.updated] Bug 42 was updated by Sam"
    );

    // Falls back to System.Title and workItemId when no message text.
    let titled = br#"{"eventType":"workitem.created",
        "resource":{"workItemId":7,"fields":{"System.Title":"New crash"}}}"#;
    let events = adapter.parse_webhook(titled).unwrap();
    assert_eq!(events[0].chat_id, "7");
    assert_eq!(events[0].text, "[workitem.created] New crash");

    // Build events are in scope.
    let build = br#"{"eventType":"build.complete","resource":{"id":99}}"#;
    assert_eq!(adapter.parse_webhook(build).unwrap().len(), 1);

    // Unrelated event types are skipped.
    let other = br#"{"eventType":"git.push","resource":{"id":1}}"#;
    assert!(adapter.parse_webhook(other).unwrap().is_empty());
}

#[test]
fn send_request_posts_comment_with_pat_as_basic_password() {
    let adapter = AzureDevOpsAdapter::new(None, None, "MY-PAT", "https://dev.azure.com/acme/");
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "42".into(),
        text: "looking".into(),
    });
    assert_eq!(
        req.url,
        "https://dev.azure.com/acme/_apis/wit/workItems/42/comments?api-version=7.0-preview.3"
    );
    assert_eq!(
        req.auth,
        SendAuth::Basic {
            username: String::new(),
            password: "MY-PAT".into()
        }
    );
    let SendBody::Json(body) = &req.body else {
        panic!("expected json body")
    };
    assert_eq!(body["text"], "looking");
}
