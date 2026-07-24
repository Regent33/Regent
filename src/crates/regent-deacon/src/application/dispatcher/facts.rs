//! Additive compatibility facts for `health` / `version` and the `update.status`
//! RPC (ADR-041, Phase 0).
//!
//! Every field added here is ADDITIVE: old JSON-RPC clients keep reading the
//! legacy `status` / `version` keys and ignore the rest; new clients read
//! `components`, `protocols`, `capabilities`, and the cached `update` verdict.
//! Nothing here is required — an absent field means "older peer", and callers
//! degrade rather than refuse the connection.
//!
//! Protocol numbers are derived from their real source of truth wherever the
//! deacon can reach it (store schema, config schema, prompt-schema marker) so
//! they can never silently drift from the thing they describe.

use super::Dispatcher;
use crate::domain::config::CURRENT_CONFIG_VERSION;
use crate::domain::entities::{RpcRequest, ok_response};
use serde_json::{Value, json};

/// This binary's version — the workspace package version, the single source of
/// truth (replaces the previously hard-coded string).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// JSON-RPC vocabulary version this deacon speaks. New/additive: absent on an
/// old peer ⇒ treat as `1`.
const DEACON_RPC_PROTOCOL: u32 = 1;

/// Live voice-call protocol. This mirrors `regent-voice-server`'s
/// `CALL_PROTOCOL`; the deacon cannot depend on that crate, so this is the one
/// deacon-side copy — the cross-language parity CI check (Phase 0 item 2, a
/// separate slice) is what guards it against drift.
const CALL_PROTOCOL: u32 = 7;

/// Prompt-schema number parsed from the real system prompt's marker line
/// (`regent-prompt-schema:vN`) so it cannot drift from the prompt itself.
fn prompt_schema() -> Option<u32> {
    regent_agent::SYSTEM_PROMPT
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("regent-prompt-schema:v"))
        .and_then(|n| n.trim().parse().ok())
}

/// Protocol facts reported together (the plan's compatibility surface).
fn protocols() -> Value {
    json!({
        "deacon_rpc": DEACON_RPC_PROTOCOL,
        "call": CALL_PROTOCOL,
        "prompt_schema": prompt_schema(),
        "store_schema": regent_store::infra::schema::SCHEMA_VERSION,
        "config_schema": CURRENT_CONFIG_VERSION,
    })
}

/// This process reports only its own component version. The caller adds its
/// own version client-side, so mixed installations are visible rather than
/// falsely assumed to match.
fn components() -> Value {
    json!({ "deacon": VERSION })
}

/// Capability tokens new peers can feature-detect (additive, string-typed).
fn capabilities() -> Value {
    json!(["update.status.v1"])
}

impl Dispatcher {
    /// `health` — an additive superset of the legacy `{status, version}` body.
    pub(super) fn health_result(&self, req: RpcRequest) {
        let mut body = json!({
            "status": "ok",
            "version": VERSION,
            "components": components(),
            "protocols": protocols(),
            "capabilities": capabilities(),
        });
        if let Some(update) = self.update_status_value() {
            body["update"] = update;
        }
        self.send(ok_response(req.id, body));
    }

    /// `version` — the legacy `{version}` body plus the same additive facts.
    pub(super) fn version_result(&self, req: RpcRequest) {
        self.send(ok_response(
            req.id,
            json!({
                "version": VERSION,
                "components": components(),
                "protocols": protocols(),
                "capabilities": capabilities(),
            }),
        ));
    }

    /// `update.status` — the cached update verdict. Never triggers a fetch; the
    /// background checker is the only thing that talks to the network.
    pub(super) fn update_status(&self, req: RpcRequest) {
        let body = self.update_status_value().unwrap_or_else(|| {
            json!({
                "current": VERSION,
                "latest": Value::Null,
                "available": false,
                "checked_at": Value::Null,
                "source": "never",
            })
        });
        self.send(ok_response(req.id, body));
    }

    /// Serialize the checker's snapshot, if a checker is wired.
    fn update_status_value(&self) -> Option<Value> {
        self.update
            .as_ref()
            .and_then(|c| serde_json::to_value(c.status()).ok())
    }
}

#[cfg(test)]
#[path = "facts_tests.rs"]
mod facts_tests;
