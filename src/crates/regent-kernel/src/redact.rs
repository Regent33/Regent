//! Secret redaction for the logging boundary (P7 security). The threat: a
//! provider/HTTP error body echoes back our request — including an `x-api-key`
//! or `Authorization` header — and that body lands in a log line. We mask the
//! known *shapes* of credentials before anything untrusted is logged.
//!
//! Two ways in, because prefix matching alone did not meet the threat above:
//!
//! 1. **Known shapes** — a token carrying an unambiguous secret prefix
//!    (`sk-ant-`, `ghp_`, `eyJ`…), masked wherever it appears.
//! 2. **Named position** — the value of a credential field (`x-api-key: …`,
//!    `password=…`) or the token after an auth scheme (`Bearer`, `Basic`),
//!    masked *whatever shape it has*.
//!
//! The second exists because the first silently missed everything it was
//! written for. `x-api-key` is named in the threat above, yet an opaque key
//! with no recognised prefix went into the log in full — verified against four
//! real shapes, all unmasked before this: a proxy key (`cpa-…`), HTTP Basic
//! (base64 of `user:password`), `password=…`, and a Google `AIza…` key.
//! Prefix lists only ever cover the vendors someone remembered.
//!
//! Still deliberately low false-positive. A field name only arms the next token
//! when it is followed by `:` or `=`, so the word "token" in ordinary prose
//! keeps the log readable.

/// Secret token prefixes, **longest/most-specific first** so masking keeps the
/// most informative recognizable prefix (`sk-ant-***`, not `sk-***`).
///
/// This list is necessarily incomplete and is the *third* line of defence, not
/// the first. The workspace reads **106** distinct credential env vars; naming
/// every vendor's format here is a race nobody wins, which is why
/// [`refresh_own_secrets`] masks by value and [`CREDENTIAL_KEYS`] masks by
/// position. What prefixes uniquely add is reach over credentials this process
/// does **not** own — a key echoed back inside a fetched page or a third
/// party's token in an API error body.
const SECRET_PREFIXES: &[&str] = &[
    "sk-ant-api03-",
    "sk-ant-",
    "sk-or-v1-",
    "sk-proj-",
    "github_pat_",
    "glpat-",
    "dckr_pat_",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xapp-",
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "ghr_",
    "AIza",   // Google (Gemini, CSE, Maps)
    "ya29.",  // Google OAuth access token
    "AKIA",   // AWS access key id
    "ASIA",   // AWS temporary access key id
    "hf_",    // Hugging Face
    "gsk_",   // Groq
    "nvapi-", // NVIDIA
    "pplx-",  // Perplexity
    "xai-",   // xAI
    "r8_",    // Replicate
    "tvly-",  // Tavily
    "fal-",   // fal.ai
    "sbp_",   // Supabase
    "figd_",  // Figma
    "npm_",
    "sk_live_",
    "rk_live_",
    "shpat_",
    "SG.", // SendGrid
    "eyJ", // JWT (base64 of `{"`)
    "sk-",
];

/// Minimum characters that must follow a prefix for it to count as a secret —
/// stops a bare `sk-` or short lookalike from being masked.
const MIN_SUFFIX: usize = 6;

/// Field names whose **value** is a credential whatever shape it has. Matched
/// only in `name:` / `name=` position, so "the token expired" stays readable.
const CREDENTIAL_KEYS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "apikey",
    "api_key",
    "x-goog-api-key",
    "x-auth-token",
    "password",
    "passwd",
    "secret",
    "client_secret",
    "access_token",
    "refresh_token",
    "id_token",
    "session_token",
    "token",
    "auth",
];

/// Auth schemes that announce the credential as the next whitespace-separated
/// token. `Basic` is here because base64 of `user:password` has no prefix to
/// recognise and is a credential in full.
const AUTH_SCHEMES: &[&str] = &["bearer", "basic", "digest"];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

fn is_credential_key(token: &str) -> bool {
    CREDENTIAL_KEYS
        .iter()
        .any(|k| token.eq_ignore_ascii_case(k))
}

fn is_auth_scheme(token: &str) -> bool {
    AUTH_SCHEMES.iter().any(|s| token.eq_ignore_ascii_case(s))
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
    // Value masking first: our own keys are matched literally, so they are
    // caught whatever shape they have and wherever they sit — including inside
    // a URL or a quoted blob the tokenizer below would never split out.
    let input = crate::redact_env::mask_own_secrets(input);
    let input = input.as_ref();
    let mut out = String::with_capacity(input.len());
    let mut token = String::new();
    // The next token is a credential — set by an auth scheme, or by a
    // credential field name that was followed by `:` or `=`.
    let mut expect_secret = false;
    // The token just emitted was a credential field name. It only arms
    // `expect_secret` once a `:` or `=` confirms it is a field and not prose.
    let mut named_key = false;

    let flush = |token: &mut String, out: &mut String, expect: &mut bool, named: &mut bool| {
        if token.is_empty() {
            return;
        }
        let prefix = secret_prefix(token);
        let scheme = is_auth_scheme(token);
        // A scheme is never itself the secret: `Authorization: Bearer sk-x`
        // must render `Bearer sk-ant-***`, not `*** sk-ant-***`.
        if prefix.is_some() || (*expect && !scheme) {
            out.push_str(&mask(prefix));
            *expect = false;
        } else {
            out.push_str(token);
        }
        if scheme {
            *expect = true;
        }
        *named = is_credential_key(token);
        token.clear();
    };

    for ch in input.chars() {
        if is_token_char(ch) {
            token.push(ch);
            continue;
        }
        flush(&mut token, &mut out, &mut expect_secret, &mut named_key);
        out.push(ch);
        if named_key && matches!(ch, ':' | '=') {
            // `x-api-key: …` / `password=…` — the field is confirmed.
            expect_secret = true;
            named_key = false;
        } else if !(ch.is_whitespace() || matches!(ch, '"' | '\'')) {
            // Whitespace and quotes sit between a name and its value in both
            // header and JSON form, so they carry the announcement across.
            // Anything else (comma, brace, newline's neighbours) ends it.
            expect_secret = false;
            named_key = false;
        }
    }
    flush(&mut token, &mut out, &mut expect_secret, &mut named_key);
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
#[path = "redact_tests.rs"]
mod tests;
