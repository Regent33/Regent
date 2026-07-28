//! Masking this process's OWN credential values. Split from `redact.rs`
//! (file-size rule).
//!
//! This is the layer that needs no vendor list. The workspace reads 106
//! distinct credential env vars; naming every vendor's key format in a prefix
//! list is a race nobody wins, and prefix matching silently missed the very
//! header `redact`'s own threat model names. Whatever shape a key has, if this
//! process is holding it, its literal value never reaches a log.
//!
//! What this cannot cover, and why the prefix list stays: credentials this
//! process does *not* own — a third party's key echoed inside a fetched page or
//! an API error body.

use std::borrow::Cow;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Env vars whose value is masked wherever it appears. Mirrors the suffixes
/// `reload_credentials_from_dotenv` already treats as credentials.
const CRED_SUFFIXES: &[&str] = &["_KEY", "_TOKEN", "_SECRET", "_PASSWORD"];

/// Shortest env value worth masking. A credential var set to `1`, `true` or
/// `local` would otherwise punch holes through every log line it appears in,
/// and an unreadable log is its own outage.
const MIN_SECRET_VALUE: usize = 12;

/// This process's own credential values. Populated on first use and refreshed
/// by [`refresh_own_secrets`], because `.env` is re-merged at runtime and a
/// snapshot taken at boot would go stale the moment a key is added.
static OWN_SECRETS: RwLock<Vec<String>> = RwLock::new(Vec::new());
static SECRETS_LOADED: AtomicBool = AtomicBool::new(false);

/// Re-reads credential env vars into the mask set; returns how many are armed.
/// Call after anything that mutates the environment.
pub fn refresh_own_secrets() -> usize {
    let mut values: Vec<String> = std::env::vars()
        .filter(|(name, value)| {
            value.chars().count() >= MIN_SECRET_VALUE && {
                let upper = name.to_ascii_uppercase();
                CRED_SUFFIXES.iter().any(|suffix| upper.ends_with(suffix))
            }
        })
        .map(|(_, value)| value)
        .collect();
    values.sort();
    values.dedup();
    // Longest first: when one credential contains another, masking the short
    // one first would leave the rest of the long one in the clear.
    values.sort_by_key(|v| std::cmp::Reverse(v.len()));
    let armed = values.len();
    if let Ok(mut guard) = OWN_SECRETS.write() {
        *guard = values;
    }
    SECRETS_LOADED.store(true, Ordering::Release);
    armed
}

/// Replaces this process's own credential values. Borrows unless one is found,
/// which is the overwhelmingly common case for a log line.
pub(crate) fn mask_own_secrets(input: &str) -> Cow<'_, str> {
    if !SECRETS_LOADED.load(Ordering::Acquire) {
        refresh_own_secrets();
    }
    let Ok(secrets) = OWN_SECRETS.read() else {
        return Cow::Borrowed(input);
    };
    let mut out = Cow::Borrowed(input);
    for secret in secrets.iter() {
        if out.contains(secret.as_str()) {
            out = Cow::Owned(out.replace(secret.as_str(), "***"));
        }
    }
    out
}
