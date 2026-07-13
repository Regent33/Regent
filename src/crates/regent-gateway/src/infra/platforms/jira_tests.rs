//! Unit tests for `jira` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn signed_mode_accepts_valid_and_rejects_invalid() {
    let adapter = JiraAdapter::new(Some("topsecret".into()), "e", "t", "https://x");
    let body = br#"{"webhookEvent":"jira:issue_created"}"#;
    assert!(adapter.verify(body, Some(&sign("topsecret", body)), None));

    // Wrong key, malformed prefix, and missing signature all fail closed.
    assert!(!adapter.verify(body, Some(&sign("wrong", body)), None));
    assert!(!adapter.verify(body, Some("deadbeef"), None));
    assert!(!adapter.verify(body, None, None));
}

#[test]
fn unsigned_mode_accepts_anything() {
    let adapter = JiraAdapter::new(None, "e", "t", "https://x");
    assert!(adapter.verify(b"{}", None, None));
    assert!(adapter.verify(b"{}", Some("sha256=whatever"), None));
}

#[test]
fn parses_issue_event_and_comment_event_and_skips_others() {
    let adapter = JiraAdapter::new(None, "e", "t", "https://x");

    let issue = br#"{"webhookEvent":"jira:issue_created",
        "issue":{"key":"PROJ-1","fields":{"summary":"Login is broken"}},
        "user":{"accountId":"acc-9"}}"#;
    let events = adapter.parse_webhook(issue).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "PROJ-1");
    assert_eq!(events[0].user_id, "acc-9");
    assert_eq!(
        events[0].text,
        "[jira:issue_created] PROJ-1: Login is broken"
    );

    let comment = br#"{"webhookEvent":"comment_created",
        "issue":{"key":"PROJ-2"},
        "comment":{"body":"Looks good to me"}}"#;
    let events = adapter.parse_webhook(comment).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "PROJ-2");
    assert_eq!(events[0].text, "[comment_created] PROJ-2: Looks good to me");

    let other = br#"{"webhookEvent":"jira:worklog_updated","issue":{"key":"PROJ-3"}}"#;
    assert!(adapter.parse_webhook(other).unwrap().is_empty());
}

#[test]
fn send_request_posts_adf_comment_with_basic_auth() {
    let adapter = JiraAdapter::new(None, "me@x.com", "API-TOKEN", "https://acme.atlassian.net/");
    let req = adapter.send_request(&OutboundMessage {
        chat_id: "PROJ-1".into(),
        text: "on it".into(),
    });
    assert_eq!(
        req.url,
        "https://acme.atlassian.net/rest/api/3/issue/PROJ-1/comment"
    );
    assert_eq!(
        req.auth,
        SendAuth::Basic {
            username: "me@x.com".into(),
            password: "API-TOKEN".into()
        }
    );
    let SendBody::Json(body) = &req.body else {
        panic!("expected json body")
    };
    assert_eq!(body["body"]["type"], "doc");
    assert_eq!(body["body"]["version"], 1);
    assert_eq!(body["body"]["content"][0]["content"][0]["text"], "on it");
}
