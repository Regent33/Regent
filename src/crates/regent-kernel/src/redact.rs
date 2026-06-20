//! Secret redaction for the logging boundary (P7 security). The threat: a
//! provider/HTTP error body echoes back our request — including an `x-api-key`
//! or `Authorization` header — and that body lands in a log line. We mask the
//! known *shapes* of credentials before anything untrusted is logged.
//!
//! Deny-by-default for known shapes, but deliberately low false-positive: we
//! only mask tokens carrying an unambiguous secret prefix, or the token right
//! after a `Bearer` keyword. Marker-based JSON value masking (e.g. arbitrary
//! `"password": "..."`) is a future widening; this covers every credential the
//! workspace actually handles (Anthropic/OpenAI/OpenRouter keys, Slack/GitHub
//! tokens, JWTs, bearer auth).

/// Secret token prefixes, **longest/most-specific first** so masking keeps the
/// most informative recognizable prefix (`sk-ant-***`, not `sk-***`).
const SECRET_PREFIXES: &[&str] = &[
    "sk-ant-api03-",
    "sk-ant-",
    "sk-or-v1-",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "xapp-",
    "ghp_",
    "gho_",
    "eyJ", // JWT (base64 of `{"`)
    "sk-",
];

/// Minimum characters that must follow a prefix for it to count as a secret —
/// stops a bare `sk-` or short lookalike from being masked.
const MIN_SUFFIX: usize = 6;

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

/// The most-specific matching secret prefix, if the token is long enough.
fn secret_prefix(token: &str) -> Option<&'static str> {
    SECRET_PREFIXES
        .iter()
        .copied()
        .find(|p| token.starts_with(p) && token.len() >= p.len() + MIN_SUFFIX)
}

fn mask(prefix: Option<&str>) -> String {
    match prefix {
        Some(p) => format!("{p}***"),
        None => "***".to_owned(),
    }
}

/// Returns `input` with secret-shaped tokens masked. Safe to call on any string
/// before logging; non-secret text is returned unchanged.
#[must_use]
pub fn redact_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut token = String::new();
    let mut prev_was_bearer = false;

    let flush = |token: &mut String, out: &mut String, prev_bearer: &mut bool| {
        if token.is_empty() {
            return;
        }
        let prefix = secret_prefix(token);
        if prefix.is_some() || *prev_bearer {
            out.push_str(&mask(prefix));
        } else {
            out.push_str(token);
        }
        // The value right after `Bearer` is the credential; only that one.
        *prev_bearer = token.eq_ignore_ascii_case("bearer");
        token.clear();
    };

    for ch in input.chars() {
        if is_token_char(ch) {
            token.push(ch);
        } else {
            flush(&mut token, &mut out, &mut prev_was_bearer);
            out.push(ch);
            // Only whitespace keeps the `Bearer <token>` adjacency; any other
            // separator (quote, comma, brace) ends it.
            if !ch.is_whitespace() {
                prev_was_bearer = false;
            }
        }
    }
    flush(&mut token, &mut out, &mut prev_was_bearer);
    out
}

/// A `std::io::Write` wrapper that redacts secrets from each write before
/// delegating — wrap a log-file writer so a leaked token never lands on disk.
/// Redaction is per write call; tracing's fmt layer emits one event per write,
/// so a secret is never split across calls in practice.
pub struct RedactingWriter<W> {
    inner: W,
}

impl<W: std::io::Write> RedactingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<W: std::io::Write> std::io::Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let redacted = redact_secrets(&String::from_utf8_lossy(buf));
        self.inner.write_all(redacted.as_bytes())?;
        // Report the original length consumed (Write contract is about input).
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn redacting_writer_masks_before_the_inner_writer_sees_bytes() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut writer = RedactingWriter::new(&mut sink);
            writeln!(writer, "auth failed for sk-ant-api03-LEAKED123456").unwrap();
        }
        let written = String::from_utf8(sink).unwrap();
        assert!(written.contains("sk-ant-api03-***"), "got: {written}");
        assert!(!written.contains("LEAKED123456"));
    }

    #[test]
    fn masks_known_provider_key_prefixes_keeping_the_prefix() {
        let got = redact_secrets(r#"{"error":"bad key sk-ant-api03-AbCdEf123456 rejected"}"#);
        assert!(got.contains("sk-ant-api03-***"), "got: {got}");
        assert!(!got.contains("AbCdEf123456"));
    }

    #[test]
    fn masks_openai_openrouter_slack_github_and_jwt() {
        for (raw, want) in [
            ("key=sk-AbCdEfGhIjKl", "sk-***"),
            ("key=sk-or-v1-AbCdEfGhIj", "sk-or-v1-***"),
            ("tok xoxb-1234567890-abcdef", "xoxb-***"),
            ("ghp_AbCdEf1234567890", "ghp_***"),
            ("eyJhbGciOiJIUzI1NiJ9abcdef", "eyJ***"),
        ] {
            let got = redact_secrets(raw);
            assert!(got.contains(want), "for {raw:?} got {got:?}");
        }
    }

    #[test]
    fn masks_the_token_after_bearer() {
        let got = redact_secrets("Authorization: Bearer abcDEF123456opaque");
        assert!(got.contains("Bearer ***"), "got: {got}");
        assert!(!got.contains("abcDEF123456opaque"));
    }

    #[test]
    fn leaves_ordinary_text_untouched() {
        let text = "tool execution failed: file /tmp/notes-2026.md not found (status 404)";
        assert_eq!(redact_secrets(text), text);
    }

    #[test]
    fn bearer_only_masks_the_immediate_next_token() {
        // A later unrelated word is not masked.
        let got = redact_secrets("Bearer sk-AbCdEfGhIj then continue normally");
        assert!(got.contains("then continue normally"), "got: {got}");
    }

    #[test]
    fn does_not_mask_a_bare_prefix() {
        assert_eq!(redact_secrets("use the sk- prefix"), "use the sk- prefix");
    }
}
