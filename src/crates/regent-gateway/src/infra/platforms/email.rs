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
mod tests {
    use super::*;

    fn body_of(params: &[(&str, &str)]) -> String {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (key, value) in params {
            serializer.append_pair(key, value);
        }
        serializer.finish()
    }

    /// Mailgun's signature: hex(HMAC-SHA256(signing_key, timestamp + token)).
    fn sign(signing_key: &str, timestamp: &str, token: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(signing_key.as_bytes()).unwrap();
        mac.update(format!("{timestamp}{token}").as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verifies_a_valid_body_signature_and_rejects_others() {
        let adapter = EmailAdapter::new("sign-key", "key-api", "mg.example.com", "bot@mg.example.com");
        let (ts, token) = ("1700000000", "abc-token");
        let sig = sign("sign-key", ts, token);
        let body = body_of(&[("timestamp", ts), ("token", token), ("signature", &sig)]);
        assert!(adapter.verify(body.as_bytes(), None, None));

        // Tampered signature.
        let tampered =
            body_of(&[("timestamp", ts), ("token", token), ("signature", "deadbeef")]);
        assert!(!adapter.verify(tampered.as_bytes(), None, None), "wrong digest → deny");

        // Wrong signing key.
        let wrong_key = sign("other-key", ts, token);
        let bad_key =
            body_of(&[("timestamp", ts), ("token", token), ("signature", &wrong_key)]);
        assert!(!adapter.verify(bad_key.as_bytes(), None, None), "wrong key → deny");

        // Missing fields → deny.
        let missing = body_of(&[("timestamp", ts), ("token", token)]);
        assert!(!adapter.verify(missing.as_bytes(), None, None), "missing signature → deny");
        assert!(!adapter.verify(b"", None, None), "empty body → deny");
    }

    #[test]
    fn parses_sender_and_body_plain_into_an_event() {
        let adapter = EmailAdapter::new("s", "k", "mg.example.com", "bot@mg.example.com");
        let body = body_of(&[
            ("sender", "alice@example.com"),
            ("subject", "Hello"),
            ("body-plain", "please help"),
        ]);
        let events = adapter.parse_webhook(body.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].chat_id, "alice@example.com");
        assert_eq!(events[0].user_id, "alice@example.com");
        assert_eq!(events[0].text, "please help");
        assert_eq!(events[0].chat_key(), "email:alice@example.com");
    }

    #[test]
    fn falls_back_to_subject_when_body_plain_is_empty() {
        let adapter = EmailAdapter::new("s", "k", "d", "f");
        let body = body_of(&[
            ("sender", "alice@example.com"),
            ("subject", "the subject"),
            ("body-plain", ""),
        ]);
        let events = adapter.parse_webhook(body.as_bytes()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text, "the subject");
    }

    #[test]
    fn skips_a_body_with_no_sender() {
        let adapter = EmailAdapter::new("s", "k", "d", "f");
        let body = body_of(&[("subject", "no sender"), ("body-plain", "orphan")]);
        assert!(adapter.parse_webhook(body.as_bytes()).unwrap().is_empty());
    }

    #[test]
    fn send_request_posts_to_messages_api_with_basic_auth_and_form_body() {
        let adapter = EmailAdapter::new("s", "key-secret", "mg.example.com", "bot@mg.example.com");
        let req = adapter.send_request(&OutboundMessage {
            chat_id: "alice@example.com".into(),
            text: "hi back".into(),
        });
        assert_eq!(req.url, "https://api.mailgun.net/v3/mg.example.com/messages");
        assert_eq!(
            req.auth,
            SendAuth::Basic { username: "api".into(), password: "key-secret".into() }
        );
        let SendBody::Form(pairs) = &req.body else { panic!("expected form body") };
        assert!(pairs.contains(&("from".to_owned(), "bot@mg.example.com".to_owned())));
        assert!(pairs.contains(&("to".to_owned(), "alice@example.com".to_owned())));
        assert!(pairs.contains(&("text".to_owned(), "hi back".to_owned())));
        assert!(pairs.contains(&("subject".to_owned(), "Re: your message".to_owned())));
    }

    #[test]
    fn signature_lives_in_the_body_not_a_header() {
        let adapter = EmailAdapter::new("s", "k", "d", "f");
        assert_eq!(adapter.signature_header(), None);
        assert_eq!(adapter.platform(), "email");
    }
}
