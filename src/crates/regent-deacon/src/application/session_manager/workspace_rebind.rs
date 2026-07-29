//! Moving a LIVE session onto a different project folder.
//!
//! The workspace used to be fixed at birth — `SessionEntry::workspace` still
//! says so for the escalation path, which must reproduce whatever the session
//! currently has. The Desktop "Open Folder" button therefore had nowhere to put
//! a folder picked mid-conversation, and started a **new chat** instead: the
//! user selected a repo and their conversation vanished from under them.
//!
//! Rebinding is the fix, and the jail is **recomputed, not edited**. Opening a
//! real repo is itself what turns the jail on (`should_sandbox`'s
//! `workspace_set` trigger, added because an unjailed session pointed at a real
//! project puts the user's home dir one bad absolute path away). So this builds
//! the context through the same constructor session birth uses, and a rebound
//! session lands in exactly the context a fresh session opened on that folder
//! would have had.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_kernel::SessionId;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Resolve a picked folder to a real directory, or say why not.
///
/// Pure and separate so the refusal path is testable without standing up a
/// whole `SessionManager`: a typo must not leave a live conversation pointed at
/// somewhere that does not exist, and that guarantee deserves a test of its own.
pub(super) fn resolve_workspace_root(root: &Path) -> Result<PathBuf, DeaconError> {
    let fail = |message: String| {
        DeaconError::Core(regent_kernel::RegentError::Tool {
            tool: "workspace.set".into(),
            message,
        })
    };
    let resolved = std::fs::canonicalize(root)
        .map_err(|e| fail(format!("cannot open {}: {e}", root.display())))?;
    if !resolved.is_dir() {
        return Err(fail(format!("{} is not a directory", resolved.display())));
    }
    Ok(resolved)
}

impl SessionManager {
    /// Repoint a live session's tools at `root`. Returns the resolved root.
    ///
    /// `Err(DeaconError::…)` when the path is not a usable directory; `Ok(None)`
    /// when the session is not live in this process, which callers surface as
    /// "unknown session" rather than guessing.
    pub async fn rebind_workspace(
        &self,
        session_id: &SessionId,
        root: &Path,
    ) -> Result<Option<PathBuf>, DeaconError> {
        // Validated BEFORE the session is touched, deliberately.
        let resolved = resolve_workspace_root(root)?;

        let (agent, approval_pending) = {
            let entries = self.entries.lock().await;
            let Some(entry) = entries.get(session_id) else {
                return Ok(None);
            };
            (
                Arc::clone(&entry.agent),
                Arc::clone(&entry.approval_pending),
            )
        };

        // The approval handler is per-session and carries the session id, so it
        // has to be rebuilt with this id rather than borrowed from birth.
        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let _ = sid_cell.set(session_id.to_string());
        let approval = self.approval_handler(&sid_cell, &approval_pending);
        // `external: false` — a rebind is a local user action through the
        // desktop shell. An externally-triggered session has no folder picker.
        let ctx = self.tool_context(false, approval, Some(&resolved));

        {
            let mut agent = agent.lock().await;
            agent.set_tool_context(ctx);
        }
        {
            let mut entries = self.entries.lock().await;
            if let Some(entry) = entries.get_mut(session_id) {
                entry.workspace = Some(resolved.clone());
            }
        }
        // Persist so a resume after restart re-opens the same tree; best-effort
        // for the same reason the birth-time stamp is.
        if let Err(e) = self
            .store
            .set_session_workspace(session_id, &resolved.display().to_string())
        {
            tracing::warn!(session = %session_id, error = %e, "workspace rebind stamp failed");
        }
        tracing::info!(session = %session_id, root = %resolved.display(), "workspace rebound");
        Ok(Some(resolved))
    }
}

#[cfg(test)]
#[path = "tests/workspace_rebind.rs"]
mod tests;
