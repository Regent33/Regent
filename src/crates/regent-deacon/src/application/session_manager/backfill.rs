//! One-shot title backfill for pre-existing untitled sessions. First-turn
//! titling only ever fires on a session's opening turn, so sessions created
//! before titling shipped stay untitled forever and the rail falls back to
//! "source · id". `backfill_titles` sweeps stored sessions and names the ones
//! that have a real exchange, reusing the SAME title-gen call as first-turn
//! titling (`SessionManager::title_for`). Cost is bounded by `limit` model
//! calls per invocation — callers repeat until `remaining` reaches 0.

use super::SessionManager;
use crate::domain::errors::DeaconError;
use regent_kernel::{Role, SessionId};
use regent_store::StoredMessage;

/// Upper bound on how many sessions a single sweep scans. Far above the real
/// backlog (~900); a plain cap avoids leaning on SQLite's negative-LIMIT quirk.
const MAX_SCAN: usize = 100_000;

/// Outcome of one `backfill_titles` sweep. `titled` + a share of `skipped` are
/// this run's work; `remaining` counts eligible sessions left untouched because
/// the `limit` was hit, so a caller can loop until it is 0.
pub struct BackfillReport {
    pub titled: usize,
    pub skipped: usize,
    pub remaining: usize,
}

impl SessionManager {
    /// Titles untitled sessions that hold a real exchange. Skips any session
    /// that already has a title, has fewer than two user/assistant messages, or
    /// whose first user message is empty/whitespace (nothing to title from).
    /// Generates + persists a title for the rest, up to `limit` model calls;
    /// eligible sessions beyond the limit are reported as `remaining` for the
    /// next call. Store errors surface verbatim; a model failure on one session
    /// counts as a skip and the sweep continues.
    pub async fn backfill_titles(&self, limit: usize) -> Result<BackfillReport, DeaconError> {
        let sessions = self.list_sessions(MAX_SCAN)?;
        let mut report = BackfillReport {
            titled: 0,
            skipped: 0,
            remaining: 0,
        };
        for meta in sessions {
            if meta.title.is_some() {
                report.skipped += 1;
                continue;
            }
            let id = SessionId::from_string(meta.id);
            let messages = self.store.get_conversation(&id).map_err(DeaconError::Store)?;
            let Some(text) = titleable_text(&messages) else {
                report.skipped += 1;
                continue;
            };
            // Eligible, but the per-call budget is spent — leave it for next time.
            if report.titled >= limit {
                report.remaining += 1;
                continue;
            }
            match self.title_for(&text).await {
                Some(title) => {
                    self.store
                        .rename_session(&id, Some(&title))
                        .map_err(DeaconError::Store)?;
                    report.titled += 1;
                }
                None => report.skipped += 1,
            }
        }
        Ok(report)
    }
}

/// The text to title a session from: the first user message's content, but only
/// when the session holds at least two user/assistant messages (a real
/// exchange) and that first message is non-empty once trimmed. `None` means the
/// session isn't worth a model call.
fn titleable_text(messages: &[StoredMessage]) -> Option<String> {
    let exchange = messages
        .iter()
        .filter(|m| matches!(m.message.role, Role::User | Role::Assistant))
        .count();
    if exchange < 2 {
        return None;
    }
    let first_user = messages
        .iter()
        .find(|m| matches!(m.message.role, Role::User))
        .and_then(|m| m.message.content.as_deref())
        .unwrap_or("");
    if first_user.trim().is_empty() {
        None
    } else {
        Some(first_user.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::titleable_text;
    use regent_kernel::ChatMessage;
    use regent_store::StoredMessage;

    fn stored(message: ChatMessage) -> StoredMessage {
        StoredMessage {
            id: 0,
            message,
            timestamp: 0.0,
            finish_reason: None,
        }
    }

    #[test]
    fn needs_two_messages_and_nonempty_first_user() {
        // Real exchange → first user text.
        let convo = vec![
            stored(ChatMessage::user("plan a trip")),
            stored(ChatMessage::assistant(Some("sure".into()), vec![])),
        ];
        assert_eq!(titleable_text(&convo).as_deref(), Some("plan a trip"));

        // Only one message → nothing to title.
        let lone = vec![stored(ChatMessage::user("hi"))];
        assert!(titleable_text(&lone).is_none());

        // Two messages but the first user turn is whitespace → skip.
        let blank = vec![
            stored(ChatMessage::user("   ")),
            stored(ChatMessage::assistant(Some("hello".into()), vec![])),
        ];
        assert!(titleable_text(&blank).is_none());
    }
}
