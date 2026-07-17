//! ADR-038 P0(b): renders the two candidate prompt profiles for
//! `profile.estimate`/`profile.report` without creating a session. Split
//! from `build.rs` (file-size rule) — same `SessionManager`, extension impl.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use std::sync::{Arc, OnceLock};

impl SessionManager {
    /// The fixed prefix a NEW session would send before any history: the
    /// rendered system prompt and the serialized tool definitions. Powers the
    /// CI prefix-ceiling gate (SPL §3.3), `context.budget`, and — via
    /// `fixed_prefix_for` — ADR-038 P0(b)'s `profile.estimate`/`profile.report`.
    /// Byte-identical to `fixed_prefix_for(false)`; kept as its own method so
    /// the CI gate's call site never has to know the `light` parameter exists.
    pub async fn fixed_prefix(&self) -> Result<(String, String), DeaconError> {
        self.fixed_prefix_for(false).await
    }

    /// Renders ONE of the two ADR-038 candidate prompt profiles for a
    /// would-be new session — `light` (minimal pinned toolset, see
    /// `build::LIGHT_PINNED`) when `true`, or today's `full` catalog when
    /// `false` — without creating a session. Same rendering path as
    /// `fixed_prefix`, so the two profiles' sizes are directly comparable.
    pub async fn fixed_prefix_for(&self, light: bool) -> Result<(String, String), DeaconError> {
        let provider = self.provider();
        let sid_cell = Arc::new(OnceLock::new());
        let (catalog, _review, ledger) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, None, None, light)
            .await?;
        let defs = serde_json::to_string(&catalog.definitions()).unwrap_or_default();
        Ok((ledger.render(), defs))
    }
}
