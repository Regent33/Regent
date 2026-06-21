//! Azure DevOps Service Hooks webhook adapter. Azure posts work-item/build
//! *events* (not chat) as JSON; we flatten each relevant event into a single
//! [`MessageEvent`] whose `chat_id` is the work-item id and whose `text` is a
//! concise human summary. The agent replies by posting a work-item comment.
//!
//! Verification: Service Hooks authenticate the *subscription* with HTTP Basic
//! over the request (no body HMAC). With a basic user+pass configured we require
//! the inbound `Authorization` header to match `Basic base64(user:pass)`,
//! compared in constant time (fail closed); unconfigured we accept and rely on a
//! secret webhook URL. The reply uses a PAT as the Basic *password* (empty
//! username). Nothing here logs the credentials or PAT.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

pub struct AzureDevOpsAdapter {
    basic_user: Option<String>,
    basic_pass: Option<String>,
    pat: String,
    org_url: String,
}

impl AzureDevOpsAdapter {
    #[must_use]
    pub fn new(
        basic_user: Option<String>,
        basic_pass: Option<String>,
        pat: impl Into<String>,
        org_url: impl Into<String>,
    ) -> Self {
        Self {
            basic_user: basic_user.filter(|u| !u.is_empty()),
            basic_pass: basic_pass.filter(|p| !p.is_empty()),
            pat: pat.into(),
            org_url: org_url.into(),
        }
    }
}

/// Constant-time byte comparison (length first, then accumulate differences).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

impl WebhookAdapter for AzureDevOpsAdapter {
    fn platform(&self) -> &str {
        "azure_devops"
    }

    fn verify(&self, _body: &[u8], signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        // Unconfigured: accept and rely on a secret webhook URL. Configured: the
        // Authorization header must equal our Basic creds.
        let (Some(user), Some(pass)) = (&self.basic_user, &self.basic_pass) else {
            return true;
        };
        let Some(sig) = signature else {
            return false;
        };
        let expected = format!("Basic {}", STANDARD.encode(format!("{user}:{pass}")));
        ct_eq(expected.as_bytes(), sig.as_bytes())
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let value: Value =
            serde_json::from_slice(body).map_err(|e| GatewayError::Parse(e.to_string()))?;
        let event = value
            .get("eventType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // Only work-item and build events; everything else is acked empty.
        if !(event.starts_with("workitem.") || event.starts_with("build.")) {
            return Ok(Vec::new());
        }
        // Prefer the rendered notification text; fall back to the work-item title.
        let summary = value
            .pointer("/message/text")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .pointer("/resource/fields/System.Title")
                    .and_then(Value::as_str)
            })
            .unwrap_or("");
        // `id` is a number for work items; `workItemId` is the fallback.
        let id = numeric_id(value.pointer("/resource/id").unwrap_or(&Value::Null))
            .or_else(|| {
                numeric_id(
                    value
                        .pointer("/resource/workItemId")
                        .unwrap_or(&Value::Null),
                )
            })
            .unwrap_or_else(|| "azure_devops".to_owned());
        Ok(vec![MessageEvent {
            platform: "azure_devops".to_owned(),
            chat_id: id,
            user_id: "azure_devops".to_owned(),
            text: format!("[{event}] {summary}"),
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        // Add a work-item comment. The PAT is the Basic *password* (empty user).
        let url = format!(
            "{}/_apis/wit/workItems/{}/comments?api-version=7.0-preview.3",
            self.org_url.trim_end_matches('/'),
            message.chat_id
        );
        SendRequest {
            url,
            auth: SendAuth::Basic {
                username: String::new(),
                password: self.pat.clone(),
            },
            body: SendBody::Json(json!({ "text": message.text })),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("authorization")
    }
}

/// Stringifies an id that may arrive as a JSON number or non-empty string.
fn numeric_id(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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
}
