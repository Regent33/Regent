//! WeChat / WeCom message crypto (WXBizMsgCrypt), isolated so the primitives
//! are unit-tested directly. Per the WeChat Open Platform spec:
//!
//! - **AESKey** = `base64(EncodingAESKey + "=")` (32 bytes); **IV** = AESKey[..16].
//! - **Decrypt:** AES-256-CBC + PKCS7 → `[16 random][4-byte BE len][message][appid]`.
//! - **Signature** (`signature` / `msg_signature`): `SHA1_hex` of the
//!   lexicographically sorted parts (`token`, `timestamp`, `nonce`, and the
//!   `Encrypt` blob for messages) concatenated.

use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha1::{Digest, Sha1};

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

/// `SHA1_hex` of the sorted parts concatenated.
#[must_use]
pub fn signature(parts: &[&str]) -> String {
    let mut sorted = parts.to_vec();
    sorted.sort_unstable();
    let mut hasher = Sha1::new();
    for part in sorted {
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Decrypts a WXBizMsgCrypt `Encrypt` blob to the inner message bytes
/// (unwrapping the `[16 random][4-byte BE len][message][appid]` envelope).
/// Fails closed on any malformed input.
#[must_use]
pub fn decrypt(encoding_aes_key: &str, encrypt_b64: &str) -> Option<Vec<u8>> {
    let key = STANDARD.decode(format!("{}=", encoding_aes_key.trim())).ok()?;
    if key.len() != 32 {
        return None;
    }
    let mut buf = STANDARD.decode(encrypt_b64.trim()).ok()?;
    let plain = Aes256CbcDec::new_from_slices(&key, &key[..16])
        .ok()?
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .ok()?;
    if plain.len() < 20 {
        return None;
    }
    let len = u32::from_be_bytes([plain[16], plain[17], plain[18], plain[19]]) as usize;
    let end = 20usize.checked_add(len)?;
    plain.get(20..end).map(<[u8]>::to_vec)
}

/// Extracts a flat XML field's value, unwrapping a `<![CDATA[...]]>` wrapper.
#[must_use]
pub fn xml_field<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let start = xml.find(&format!("<{tag}>"))? + tag.len() + 2;
    let end = xml[start..].find(&format!("</{tag}>"))? + start;
    let inner = xml[start..end].trim();
    Some(inner.strip_prefix("<![CDATA[").and_then(|s| s.strip_suffix("]]>")).unwrap_or(inner))
}

#[cfg(test)]
pub(crate) fn encrypt(encoding_aes_key: &str, message: &[u8], appid: &str) -> String {
    use aes::cipher::{BlockEncryptMut, block_padding::Pkcs7};
    type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
    let key = STANDARD.decode(format!("{}=", encoding_aes_key.trim())).unwrap();
    let mut plain = Vec::new();
    plain.extend_from_slice(&[7u8; 16]); // 16 "random" bytes
    plain.extend_from_slice(
        &u32::try_from(message.len()).expect("message length fits u32").to_be_bytes(),
    );
    plain.extend_from_slice(message);
    plain.extend_from_slice(appid.as_bytes());
    let plain_len = plain.len();
    let mut buf = plain;
    buf.resize(plain_len + 16, 0);
    let ciphertext = Aes256CbcEnc::new_from_slices(&key, &key[..16])
        .unwrap()
        .encrypt_padded_mut::<Pkcs7>(&mut buf, plain_len)
        .unwrap();
    STANDARD.encode(ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aes_key() -> String {
        // A valid 43-char EncodingAESKey: base64 of 32 bytes minus the trailing '='.
        STANDARD.encode([42u8; 32]).trim_end_matches('=').to_owned()
    }

    #[test]
    fn signature_is_sorted_sha1() {
        let sig = signature(&["zzz", "aaa", "mmm"]);
        let mut h = Sha1::new();
        h.update(b"aaammmzzz");
        assert_eq!(sig, hex::encode(h.finalize()));
    }

    #[test]
    fn decrypt_round_trips_and_fails_closed() {
        let key = aes_key();
        let blob = encrypt(&key, b"hello wechat", "wxappid");
        assert_eq!(decrypt(&key, &blob).unwrap(), b"hello wechat");
        assert!(decrypt(&key, "not-base64!!!").is_none());
        assert!(decrypt("short", &blob).is_none());
    }

    #[test]
    fn xml_field_handles_cdata_and_plain() {
        let xml = "<xml><Encrypt><![CDATA[abc]]></Encrypt><MsgType>text</MsgType></xml>";
        assert_eq!(xml_field(xml, "Encrypt"), Some("abc"));
        assert_eq!(xml_field(xml, "MsgType"), Some("text"));
        assert_eq!(xml_field(xml, "Missing"), None);
    }
}
