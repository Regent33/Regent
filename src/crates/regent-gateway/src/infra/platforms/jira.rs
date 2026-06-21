//! Jira Cloud webhook adapter. Jira posts issue/comment *events* (not chat) as
//! JSON; we flatten each relevant event into a single [`MessageEvent`] whose
//! `chat_id` is the issue key and whose `text` is a concise human summary. The
//! agent replies by posting a comment via REST v3 (Atlassian Document Format).
//!
//! Verification: Jira can sign the raw body as `X-Hub-Signature: sha256=<hex>`
//! (HMAC-SHA256). With a secret configured we require and constant-time-check it
//! (fail closed); Jira webhooks are often *unsigned*, so with no secret we
//! accept and rely on a secret webhook URL. Nothing here logs the secret/token.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct JiraAdapter {
    webhook_secret: Option<String>,
    email: String,
    api_token: String,
    base_url: String,
}

impl JiraAdapter {
    #[must_use]
    pub fn new(
        webhook_secret: Option<String>,
        email: impl Into<String>,
        api_token: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            webhook_secret: webhook_secret.filter(|s| !s.is_empty()),
            email: email.into(),
            api_token: api_token.into(),
            base_url: base_url.into(),
        }
    }
}

impl WebhookAdapter for JiraAdapter {
    fn platform(&self) -> &str {
        "jira"
    }

    fn verify(&self, body: &[u8], signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        // Unsigned mode (no shared secret): Jira webhooks are often unsigned, so
        // accept and rely on the webhook URL being secret.
        let Some(secret) = &self.webhook_secret else {
            return true;
        };
        // Signed mode: require `sha256=<hex>` and constant-time verify it.
        let Some(hex_part) = signature.and_then(|s| s.strip_prefix("sha256=")) else {
            return false;
        };
        let Ok(expected) = hex::decode(hex_part) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
            return false;
        };
        mac.update(body);
        mac.verify_slice(&expected).is_ok() // constant-time
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        let event = value
            .get("webhookEvent")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let has_comment = value.pointer("/comment").is_some();
        let interesting = matches!(
            event,
            "jira:issue_created" | "jira:issue_updated" | "comment_created"
        ) || has_comment;
        if !interesting {
            return Ok(Vec::new());
        }
        let key = value
            .pointer("/issue/key")
            .and_then(Value::as_str)
            .unwrap_or("jira");
        // Comment events summarize the comment body; issue events the summary.
        let detail = if has_comment {
            value.pointer("/comment/body").and_then(Value::as_str)
        } else {
            value
                .pointer("/issue/fields/summary")
                .and_then(Value::as_str)
        }
        .unwrap_or("");
        let user = value
            .pointer("/user/accountId")
            .and_then(Value::as_str)
            .unwrap_or("jira");
        Ok(vec![MessageEvent {
            platform: "jira".to_owned(),
            chat_id: key.to_owned(),
            user_id: user.to_owned(),
            text: format!("[{event}] {key}: {detail}"),
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        // Jira Cloud REST v3 requires the comment body in Atlassian Document
        // Format (ADF). Auth is HTTP Basic: email + API token.
        let url = format!(
            "{}/rest/api/3/issue/{}/comment",
            self.base_url.trim_end_matches('/'),
            message.chat_id
        );
        SendRequest {
            url,
            auth: SendAuth::Basic {
                username: self.email.clone(),
                password: self.api_token.clone(),
            },
            body: SendBody::Json(json!({
                "body": {
                    "type": "doc",
                    "version": 1,
                    "content": [{
                        "type": "paragraph",
                        "content": [{ "type": "text", "text": message.text }]
                    }]
                }
            })),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("x-hub-signature")
    }
}

#[cfg(test)]
mod tests {
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
        let adapter =
            JiraAdapter::new(None, "me@x.com", "API-TOKEN", "https://acme.atlassian.net/");
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
}
