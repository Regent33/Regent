//! Authorization policy — the evaluation order, default deny:
//! allow-all flag → allowlist → paired users → deny. Pairing: an
//! authorized user issues a one-time code; an unknown user redeems it by
//! sending the bare code.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

/// Serializable snapshot for persistence at the composition root.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthSnapshot {
    pub allow_all: bool,
    /// `platform:user_id` entries (configured operators).
    pub allowlist: HashSet<String>,
    /// Users authorized at runtime via pairing codes.
    pub paired: HashSet<String>,
}

pub struct AuthPolicy {
    state: Mutex<AuthState>,
}

struct AuthState {
    snapshot: AuthSnapshot,
    pending_codes: HashSet<String>,
}

impl AuthPolicy {
    #[must_use]
    pub fn new(snapshot: AuthSnapshot) -> Self {
        Self {
            state: Mutex::new(AuthState {
                snapshot,
                pending_codes: HashSet::new(),
            }),
        }
    }

    #[must_use]
    pub fn is_authorized(&self, user_key: &str) -> bool {
        let state = self.state.lock().expect("auth mutex poisoned");
        state.snapshot.allow_all
            || state.snapshot.allowlist.contains(user_key)
            || state.snapshot.paired.contains(user_key)
    }

    /// Issued by an already-authorized user (`/pair`). One-time use.
    #[must_use]
    pub fn create_pairing_code(&self) -> String {
        let code = format!(
            "PAIR-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8].to_uppercase()
        );
        self.state
            .lock()
            .expect("auth mutex poisoned")
            .pending_codes
            .insert(code.clone());
        code
    }

    /// An unknown user sending a valid code becomes paired (code consumed).
    pub fn try_redeem_code(&self, text: &str, user_key: &str) -> bool {
        let mut state = self.state.lock().expect("auth mutex poisoned");
        if state.pending_codes.remove(text.trim()) {
            state.snapshot.paired.insert(user_key.to_owned());
            tracing::info!(user = user_key, "user paired");
            true
        } else {
            false
        }
    }

    /// For persistence after pairing changes.
    #[must_use]
    pub fn snapshot(&self) -> AuthSnapshot {
        self.state
            .lock()
            .expect("auth mutex poisoned")
            .snapshot
            .clone()
    }
}

/// Loads the persisted pairing snapshot from `<home>/gateway-auth.json`, then
/// overlays operator config from the environment (config is the source of truth
/// on every boot, so the allowlist/allow_all always reflect current env).
///
/// Platform-agnostic: `REGENT_ALLOW_ALL` and `REGENT_ALLOWED_USERS` (comma-
/// separated `platform:user_id`, e.g. `slack:U123,discord:456`). The legacy
/// Telegram vars (`REGENT_TELEGRAM_ALLOW_ALL`, `REGENT_TELEGRAM_ALLOWED_USERS`
/// as bare ids) are still honored as aliases so existing setups don't break.
#[must_use]
pub fn load_auth_snapshot(home: &Path) -> AuthSnapshot {
    let mut snapshot: AuthSnapshot = std::fs::read_to_string(home.join("gateway-auth.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    let flag =
        |k: &str| std::env::var(k).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    snapshot.allow_all = flag("REGENT_ALLOW_ALL") || flag("REGENT_TELEGRAM_ALLOW_ALL");
    let split = |v: String| -> Vec<String> {
        v.split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    };
    // Generalized `platform:id` entries, plus legacy bare telegram ids.
    let mut allow: HashSet<String> =
        split(std::env::var("REGENT_ALLOWED_USERS").unwrap_or_default())
            .into_iter()
            .collect();
    allow.extend(
        split(std::env::var("REGENT_TELEGRAM_ALLOWED_USERS").unwrap_or_default())
            .into_iter()
            .map(|id| format!("telegram:{id}")),
    );
    snapshot.allowlist = allow;
    snapshot
}

/// Atomically persists the snapshot (tmp + rename) so a crash mid-write can't
/// corrupt `gateway-auth.json`. Called after a pairing change.
pub fn persist_auth_snapshot(home: &Path, snapshot: &AuthSnapshot) -> std::io::Result<()> {
    let path = home.join("gateway-auth.json");
    let tmp = home.join("gateway-auth.json.tmp");
    let body = serde_json::to_string_pretty(snapshot).unwrap_or_default();
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(allowlist: &[&str]) -> AuthPolicy {
        AuthPolicy::new(AuthSnapshot {
            allow_all: false,
            allowlist: allowlist.iter().map(|s| (*s).to_owned()).collect(),
            paired: HashSet::new(),
        })
    }

    #[test]
    fn default_deny_allowlist_and_pairing_flow() {
        let auth = policy(&["telegram:1"]);
        assert!(auth.is_authorized("telegram:1"));
        assert!(!auth.is_authorized("telegram:2"));

        // wrong code does nothing
        assert!(!auth.try_redeem_code("PAIR-NOPE", "telegram:2"));
        assert!(!auth.is_authorized("telegram:2"));

        // issued code pairs exactly once
        let code = auth.create_pairing_code();
        assert!(auth.try_redeem_code(&code, "telegram:2"));
        assert!(auth.is_authorized("telegram:2"));
        assert!(
            !auth.try_redeem_code(&code, "telegram:3"),
            "codes are one-time"
        );
        assert!(auth.snapshot().paired.contains("telegram:2"));
    }

    #[test]
    fn allow_all_short_circuits() {
        let auth = AuthPolicy::new(AuthSnapshot {
            allow_all: true,
            ..AuthSnapshot::default()
        });
        assert!(auth.is_authorized("anything:anyone"));
    }

    #[test]
    fn persist_then_load_round_trips_paired_users() {
        // Atomic persist (tmp + rename) then reload; paired users survive. The
        // env overlay only touches allow_all/allowlist, so `paired` is stable.
        let dir = std::env::temp_dir().join(format!("regent-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let snap = AuthSnapshot {
            allow_all: false,
            allowlist: HashSet::new(),
            paired: ["slack:U1".to_owned(), "discord:42".to_owned()]
                .into_iter()
                .collect(),
        };
        persist_auth_snapshot(&dir, &snap).unwrap();
        let loaded = load_auth_snapshot(&dir);
        assert!(loaded.paired.contains("slack:U1"));
        assert!(loaded.paired.contains("discord:42"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
