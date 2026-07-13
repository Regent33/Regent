//! Inbound email adapter over Mailgun. Mailgun's Inbound-Parse route POSTs each
//! received email as `application/x-www-form-urlencoded`, with the webhook
//! signature carried **in the body** (`timestamp`, `token`, `signature`) rather
//! than a header — so `signature_header` is `None` and `verify` reads the form.
//! The signature is HMAC-SHA256(signing_key, `timestamp + token`), hex-encoded.
//! Replies go out via the Mailgun Messages REST API with HTTP Basic auth
//! (username `api`) and a form body. Parse/verify/build are pure — unit-testable
//! without a live account; only the send needs credentials.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct EmailAdapter {
    signing_key: String,
    api_key: String,
    domain: String,
    from_address: String,
}

impl EmailAdapter {
    #[must_use]
    pub fn new(
        signing_key: impl Into<String>,
        api_key: impl Into<String>,
        domain: impl Into<String>,
        from_address: impl Into<String>,
    ) -> Self {
        Self {
            signing_key: signing_key.into(),
            api_key: api_key.into(),
            domain: domain.into(),
            from_address: from_address.into(),
        }
    }
}

impl WebhookAdapter for EmailAdapter {
    fn platform(&self) -> &str {
        "email"
    }

    /// Mailgun's webhook proof rides in the POST body, not a header. Parse the
    /// form for `timestamp`/`token`/`signature` and check
    /// HMAC-SHA256(signing_key, timestamp+token) == signature (constant-time,
    /// fail-closed). The header args are unused.
    fn verify(&self, body: &[u8], _signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        let mut timestamp = None;
        let mut token = None;
        let mut signature = None;
        for (key, value) in form_urlencoded::parse(body) {
            match key.as_ref() {
                "timestamp" => timestamp = Some(value.into_owned()),
                "token" => token = Some(value.into_owned()),
                "signature" => signature = Some(value.into_owned()),
                _ => {}
            }
        }
        let (Some(timestamp), Some(token), Some(signature)) = (timestamp, token, signature) else {
            return false; // missing fields → deny
        };
        let Ok(expected) = hex::decode(&signature) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(self.signing_key.as_bytes()) else {
            return false;
        };
        mac.update(format!("{timestamp}{token}").as_bytes());
        mac.verify_slice(&expected).is_ok() // constant-time
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let mut sender = None;
        let mut body_plain = None;
        let mut subject = None;
        for (key, value) in form_urlencoded::parse(body) {
            match key.as_ref() {
                "sender" => sender = Some(value.into_owned()),
                "body-plain" => body_plain = Some(value.into_owned()),
                "subject" => subject = Some(value.into_owned()),
                _ => {}
            }
        }
        // Fall back to the subject when the plain-text body is missing/empty.
        let text = body_plain
            .filter(|t| !t.is_empty())
            .or(subject)
            .filter(|t| !t.is_empty());
        let (Some(sender), Some(text)) = (sender, text) else {
            return Ok(Vec::new());
        };
        Ok(vec![MessageEvent {
            platform: "email".to_owned(),
            chat_id: sender.clone(),
            user_id: sender,
            text,
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: format!("https://api.mailgun.net/v3/{}/messages", self.domain),
            auth: SendAuth::Basic {
                username: "api".to_owned(),
                password: self.api_key.clone(),
            },
            body: SendBody::Form(vec![
                ("from".to_owned(), self.from_address.clone()),
                ("to".to_owned(), message.chat_id.clone()),
                ("subject".to_owned(), "Re: your message".to_owned()),
                ("text".to_owned(), message.text.clone()),
            ]),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        None // Mailgun signs in the body, not a header.
    }
}

#[cfg(test)]
#[path = "email_tests.rs"]
mod tests;
