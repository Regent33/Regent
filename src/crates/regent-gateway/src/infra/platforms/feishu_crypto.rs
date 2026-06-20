//! Feishu/Lark event-callback crypto, isolated from the adapter so the
//! primitives are unit-tested directly. Per the Feishu Open Platform spec:
//!
//! - **Decrypt:** `key = SHA256(encrypt_key)`; the `encrypt` field is
//!   `base64(iv ‖ ciphertext)` with a 16-byte IV prefix; AES-256-CBC + PKCS7.
//! - **Signature** (`X-Lark-Signature`): `SHA256_hex(timestamp ‖ nonce ‖
//!   encrypt_key ‖ raw_body)`.

use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

/// Decrypts a Feishu `encrypt` payload to its plaintext JSON bytes. Returns
/// `None` on any malformed input (bad base64, short IV, bad padding/key) —
/// fails closed, never panics.
#[must_use]
pub fn decrypt(encrypt_key: &str, encrypt_b64: &str) -> Option<Vec<u8>> {
    let key = Sha256::digest(encrypt_key.as_bytes()); // 32-byte AES-256 key
    let data = STANDARD.decode(encrypt_b64).ok()?;
    if data.len() < 16 {
        return None;
    }
    let (iv, ciphertext) = data.split_at(16);
    let mut buf = ciphertext.to_vec();
    let cipher = Aes256CbcDec::new_from_slices(&key, iv).ok()?;
    let plaintext = cipher.decrypt_padded_mut::<Pkcs7>(&mut buf).ok()?;
    Some(plaintext.to_vec())
}

/// `SHA256_hex(timestamp ‖ nonce ‖ encrypt_key ‖ body)`.
#[must_use]
pub fn sign(timestamp: &str, nonce: &str, encrypt_key: &str, body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(timestamp.as_bytes());
    hasher.update(nonce.as_bytes());
    hasher.update(encrypt_key.as_bytes());
    hasher.update(body);
    hex::encode(hasher.finalize())
}

/// Constant-time byte comparison (no early-exit timing leak on signatures).
#[must_use]
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
pub(crate) fn encrypt(encrypt_key: &str, plaintext: &[u8], iv: &[u8; 16]) -> String {
    use aes::cipher::{BlockEncryptMut, block_padding::Pkcs7};
    type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
    let key = Sha256::digest(encrypt_key.as_bytes());
    let mut buf = vec![0u8; plaintext.len() + 16];
    buf[..plaintext.len()].copy_from_slice(plaintext);
    let ciphertext = Aes256CbcEnc::new_from_slices(&key, iv)
        .unwrap()
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
        .unwrap();
    let mut blob = iv.to_vec();
    blob.extend_from_slice(ciphertext);
    STANDARD.encode(blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrypt_round_trips_and_fails_closed() {
        let key = "my-encrypt-key";
        let plain = br#"{"type":"url_verification","challenge":"abc123"}"#;
        let blob = encrypt(key, plain, &[9u8; 16]);
        assert_eq!(decrypt(key, &blob).unwrap(), plain);

        // Wrong key never reproduces the plaintext; garbage input is None.
        assert_ne!(decrypt("wrong-key", &blob), Some(plain.to_vec()));
        assert!(decrypt(key, "not base64!!!").is_none());
        assert!(decrypt(key, "").is_none());
    }

    #[test]
    fn signature_matches_reference_formula() {
        let sig = sign("ts", "nonce", "key", b"BODY");
        let mut h = Sha256::new();
        h.update(b"tsnoncekeyBODY"); // timestamp ‖ nonce ‖ encrypt_key ‖ body
        assert_eq!(sig, hex::encode(h.finalize()));
        assert_eq!(sig.len(), 64);
    }

    #[test]
    fn ct_eq_basics() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
    }
}
