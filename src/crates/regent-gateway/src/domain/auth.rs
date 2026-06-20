//! Authorization policy — the Hermes evaluation order, default deny:
//! allow-all flag → allowlist → paired users → deny. Pairing: an
//! authorized user issues a one-time code; an unknown user redeems it by
//! sending the bare code.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
            state: Mutex::new(AuthState { snapshot, pending_codes: HashSet::new() }),
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
        let code = format!("PAIR-{}", &uuid::Uuid::new_v4().simple().to_string()[..8].to_uppercase());
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
        self.state.lock().expect("auth mutex poisoned").snapshot.clone()
    }
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
        assert!(!auth.try_redeem_code(&code, "telegram:3"), "codes are one-time");
        assert!(auth.snapshot().paired.contains("telegram:2"));
    }

    #[test]
    fn allow_all_short_circuits() {
        let auth = AuthPolicy::new(AuthSnapshot { allow_all: true, ..AuthSnapshot::default() });
        assert!(auth.is_authorized("anything:anyone"));
    }
}
