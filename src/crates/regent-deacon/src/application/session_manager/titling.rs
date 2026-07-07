//! First-turn session titling: one cheap aux model call turns the opening user
//! message into a short (<=6-word) title, stored via `rename_session` and
//! announced with a `session.titled` notification. Best-effort throughout —
//! it runs detached from the turn and only ever `warn!`s on failure.

use super::SessionManager;
use crate::domain::entities::RpcNotification;
use regent_kernel::{ChatMessage, Role, SessionId};
use regent_providers::ChatRequest;
use serde_json::json;

const TITLE_SYSTEM: &str = "You write a short, specific title for a chat from the \
    user's first message. Reply with ONLY the title: at most 6 words, no quotes, \
    no trailing punctuation, no preamble.";

impl SessionManager {
    /// Should this turn generate a title? Only when the session has no title yet
    /// and no prior user turn exists (i.e. the turn about to run is its first).
    /// Extracted as a pure function so the gate is unit-testable.
    #[must_use]
    pub(crate) fn should_generate_title(has_title: bool, prior_user_turns: usize) -> bool {
        !has_title && prior_user_turns == 0
    }

    /// Whether the session already has a stored title. Missing session → false.
    #[must_use]
    pub(crate) fn session_has_title(&self, id: &SessionId) -> bool {
        self.store
            .session_meta(id)
            .ok()
            .and_then(|m| m.title)
            .is_some()
    }

    /// Count of user-role messages already stored. `0` means the current turn is
    /// the session's first user turn. Store errors count as `0` (titling is
    /// best-effort — better to attempt than to skip on a transient read error).
    #[must_use]
    pub(crate) fn prior_user_turns(&self, id: &SessionId) -> usize {
        self.store
            .get_conversation(id)
            .map(|msgs| {
                msgs.iter()
                    .filter(|m| matches!(m.message.role, Role::User))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Generate + store a title for `session_id`'s first turn, then emit
    /// `session.titled {session_id, title}`. Re-checks the untitled invariant
    /// under fresh state (guards against a race with a concurrent rename). Any
    /// failure is logged and swallowed — never surfaced to the turn.
    pub async fn generate_title(&self, session_id: SessionId, first_user_text: String) {
        if self.session_has_title(&session_id) {
            return;
        }
        let provider = self.provider();
        let mut request = ChatRequest::new(TITLE_SYSTEM, vec![ChatMessage::user(&first_user_text)]);
        request.max_tokens = Some(24);
        let raw = match provider.complete(&request).await {
            Ok(resp) => resp.message.content.unwrap_or_default(),
            Err(error) => {
                tracing::warn!(%error, session = %session_id, "title generation call failed");
                return;
            }
        };
        let title = clean_title(&raw);
        if title.is_empty() {
            return;
        }
        match self.store.rename_session(&session_id, Some(&title)) {
            Ok(_) => {
                let notif = RpcNotification::new(
                    "session.titled",
                    json!({ "session_id": session_id.to_string(), "title": title }),
                );
                if let Ok(line) = serde_json::to_string(&notif) {
                    self.out_tx.send(line).ok();
                }
            }
            Err(error) => tracing::warn!(%error, session = %session_id, "storing title failed"),
        }
    }
}

/// Normalize a model's title reply to a clean, short line: first non-empty line,
/// stripped of surrounding quotes/backticks, capped at 6 words, trailing
/// sentence punctuation removed.
fn clean_title(raw: &str) -> String {
    let line = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let stripped = line.trim_matches(|c| c == '"' || c == '\'' || c == '`' || c == '*');
    let words: Vec<&str> = stripped.split_whitespace().take(6).collect();
    words
        .join(" ")
        .trim_end_matches(['.', ',', '!', '?', ':', ';'])
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{SessionManager, clean_title};

    #[test]
    fn title_gate_only_fires_untitled_first_turn() {
        assert!(SessionManager::should_generate_title(false, 0));
        // Already titled → never.
        assert!(!SessionManager::should_generate_title(true, 0));
        // Has prior user turns → not the first turn.
        assert!(!SessionManager::should_generate_title(false, 1));
        assert!(!SessionManager::should_generate_title(true, 3));
    }

    #[test]
    fn clean_title_trims_and_caps() {
        assert_eq!(clean_title("\"Fix the login bug\""), "Fix the login bug");
        assert_eq!(
            clean_title("Plan a road trip across seven states now"),
            "Plan a road trip across seven"
        );
        assert_eq!(clean_title("Deploy the API!"), "Deploy the API");
        assert_eq!(clean_title("  \n  Refactor  \n more"), "Refactor");
        assert_eq!(clean_title("   "), "");
        assert_eq!(clean_title("`quarterly report`"), "quarterly report");
    }
}
