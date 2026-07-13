//! Unit tests for `google_chat` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{EncodingKey, Header, encode};
use rand_core::OsRng;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;

const KID: &str = "test-kid-1";

impl GoogleChatAdapter {
    fn insert_key(&self, kid: &str, key: DecodingKey) {
        self.keys.write().unwrap().insert(kid.to_owned(), key);
    }
}

fn now() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
}

/// Mints a test RSA keypair → (signing key, the matching `DecodingKey` built
/// the production way from base64url `n`/`e` JWK components).
fn keypair() -> (EncodingKey, DecodingKey) {
    let private = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
    let public = private.to_public_key();
    let enc = EncodingKey::from_rsa_pem(
        private
            .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    // Build the DecodingKey the production way: from base64url JWK n/e.
    let n = URL_SAFE_NO_PAD.encode(public.n().to_bytes_be());
    let e = URL_SAFE_NO_PAD.encode(public.e().to_bytes_be());
    (enc, DecodingKey::from_rsa_components(&n, &e).unwrap())
}

fn token(enc: &EncodingKey, iss: &str, aud: &str, exp: usize) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_owned());
    encode(&header, &json!({ "iss": iss, "aud": aud, "exp": exp }), enc).unwrap()
}

#[test]
fn verifies_a_valid_google_signed_jwt() {
    let (enc, dec) = keypair();
    let adapter = GoogleChatAdapter::new("1234567890");
    adapter.insert_key(KID, dec);
    let jwt = token(&enc, ISSUER, "1234567890", now() + 3600);
    assert!(adapter.verify(b"", Some(&format!("Bearer {jwt}")), None));
    // Bare token (no scheme) is also accepted.
    assert!(adapter.verify(b"", Some(&jwt), None));
}

#[test]
fn rejects_wrong_audience_issuer_expiry_and_unknown_key() {
    let (enc, dec) = keypair();
    let adapter = GoogleChatAdapter::new("1234567890");
    adapter.insert_key(KID, dec);

    assert!(
        !adapter.verify(b"", Some(&token(&enc, ISSUER, "9999", now() + 3600)), None),
        "aud"
    );
    assert!(
        !adapter.verify(
            b"",
            Some(&token(&enc, "evil@x", "1234567890", now() + 3600)),
            None
        ),
        "iss"
    );
    assert!(
        !adapter.verify(
            b"",
            Some(&token(&enc, ISSUER, "1234567890", now() - 7200)),
            None
        ),
        "exp"
    );
    assert!(!adapter.verify(b"", None, None), "missing header");
    assert!(
        !adapter.verify(b"", Some("Bearer not.a.jwt"), None),
        "garbage"
    );

    // Cache without the token's kid → deny (covers cold cache / rotation).
    let cold = GoogleChatAdapter::new("1234567890");
    assert!(!cold.verify(
        b"",
        Some(&token(&enc, ISSUER, "1234567890", now() + 3600)),
        None
    ));
}

#[test]
fn parses_message_event_and_renders_sync_reply() {
    let adapter = GoogleChatAdapter::new("1234567890");
    let body = br#"{"type":"MESSAGE","message":{"text":"hello bot",
        "sender":{"name":"users/111"},"space":{"name":"spaces/AAA"}}}"#;
    let events = adapter.parse_webhook(body).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].chat_id, "spaces/AAA");
    assert_eq!(events[0].user_id, "users/111");
    assert_eq!(events[0].text, "hello bot");

    assert!(
        adapter
            .parse_webhook(br#"{"type":"ADDED_TO_SPACE"}"#)
            .unwrap()
            .is_empty()
    );

    assert!(adapter.sync_reply());
    let SyncReply::Json(reply) = adapter.sync_response("hi back") else {
        panic!("json")
    };
    assert_eq!(reply["text"], "hi back");
}
