//! Shared Twilio request-signature verification, used by both the SMS and Voice
//! adapters. Twilio sets `X-Twilio-Signature` = base64(HMAC-SHA1(authToken,
//! requestUrl + each POST param sorted by key and concatenated as `key||value`)).
//! The comparison is constant-time. Any malformed input fails closed.

use crate::domain::contracts::WebhookRequest;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

pub(crate) fn verify_signature(auth_token: &str, request: &WebhookRequest<'_>) -> bool {
    let Some(signature) = request.signature else {
        return false;
    };
    let Ok(expected) = STANDARD.decode(signature) else {
        return false;
    };
    let mut params: Vec<(String, String)> = form_urlencoded::parse(request.body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    params.sort_by(|a, b| a.0.cmp(&b.0));
    let Ok(mut mac) = HmacSha1::new_from_slice(auth_token.as_bytes()) else {
        return false;
    };
    mac.update(request.url.as_bytes());
    for (key, value) in &params {
        mac.update(key.as_bytes());
        mac.update(value.as_bytes());
    }
    mac.verify_slice(&expected).is_ok() // constant-time
}

#[cfg(test)]
pub(crate) fn sign(auth_token: &str, url: &str, params: &[(&str, &str)]) -> String {
    let mut sorted = params.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut mac = HmacSha1::new_from_slice(auth_token.as_bytes()).unwrap();
    mac.update(url.as_bytes());
    for (key, value) in &sorted {
        mac.update(key.as_bytes());
        mac.update(value.as_bytes());
    }
    STANDARD.encode(mac.finalize().into_bytes())
}
