//! Twilio SMS webhook adapter. Inbound messages arrive as
//! `application/x-www-form-urlencoded` POSTs. Twilio signs the request with
//! `X-Twilio-Signature` = base64(HMAC-SHA1(authToken, requestUrl + sorted
//! params)) — note the signature covers the **URL and params**, not the body
//! alone, so verification runs through `verify_request` (not `verify`). Replies
//! go out via the Messages REST API with HTTP Basic auth and a form body.
//! Parse/verify/build are pure — unit-testable without a live account.

use crate::domain::contracts::{SendAuth, SendBody, SendRequest, WebhookAdapter, WebhookRequest};
use crate::domain::entities::{MessageEvent, OutboundMessage};
use crate::domain::errors::GatewayError;

pub struct TwilioSmsAdapter {
    account_sid: String,
    auth_token: String,
    from_number: String,
}

impl TwilioSmsAdapter {
    #[must_use]
    pub fn new(
        account_sid: impl Into<String>,
        auth_token: impl Into<String>,
        from_number: impl Into<String>,
    ) -> Self {
        Self {
            account_sid: account_sid.into(),
            auth_token: auth_token.into(),
            from_number: from_number.into(),
        }
    }
}

impl WebhookAdapter for TwilioSmsAdapter {
    fn platform(&self) -> &str {
        "twilio_sms"
    }

    /// Twilio's signature covers the request URL, not the body alone, so the
    /// body-only path can't verify it — deny by default (the route uses
    /// `verify_request`).
    fn verify(&self, _body: &[u8], _signature: Option<&str>, _timestamp: Option<&str>) -> bool {
        false
    }

    fn verify_request(&self, request: &WebhookRequest<'_>) -> bool {
        super::twilio::verify_signature(&self.auth_token, request)
    }

    fn parse_webhook(&self, body: &[u8]) -> Result<Vec<MessageEvent>, GatewayError> {
        let mut from = None;
        let mut text = None;
        for (key, value) in form_urlencoded::parse(body) {
            match key.as_ref() {
                "From" => from = Some(value.into_owned()),
                "Body" => text = Some(value.into_owned()),
                _ => {}
            }
        }
        // Status callbacks and other non-message posts have no Body/From.
        let (Some(from), Some(text)) = (from, text) else {
            return Ok(Vec::new());
        };
        Ok(vec![MessageEvent {
            platform: "twilio_sms".to_owned(),
            chat_id: from.clone(),
            user_id: from,
            text,
        }])
    }

    fn send_request(&self, message: &OutboundMessage) -> SendRequest {
        SendRequest {
            url: format!(
                "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
                self.account_sid
            ),
            auth: SendAuth::Basic {
                username: self.account_sid.clone(),
                password: self.auth_token.clone(),
            },
            body: SendBody::Form(vec![
                ("From".to_owned(), self.from_number.clone()),
                ("To".to_owned(), message.chat_id.clone()),
                ("Body".to_owned(), message.text.clone()),
            ]),
        }
    }

    fn signature_header(&self) -> Option<&str> {
        Some("x-twilio-signature")
    }
}

#[cfg(test)]
#[path = "twilio_sms_tests.rs"]
mod tests;
