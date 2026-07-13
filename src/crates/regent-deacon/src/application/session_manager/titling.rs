//! First-turn session titling: one cheap aux model call turns the opening user
//! message into a short (<=6-word) title, stored via `rename_session` and
//! announced with a `session.titled` notification. Best-effort throughout —
//! it runs detached from the turn and only ever `warn!`s on failure.

use super::SessionManager;
use crate::domain::entities::RpcNotification;
use regent_kernel::{ChatMessage, Role, SessionId};
use regent_providers::ChatRequest;
use serde_json::json;

const TITLE_SYSTEM: &str = "You write a short, specific title for a chat from its \
    opening exchange. Name the TOPIC, never the greeting — if the user opens with \
    a bare hello, title what the conversation turned out to be about. Reply with \
    ONLY the title: at most 6 words, no quotes, no trailing punctuation, no preamble.";

/// Cap per side of [`exchange_snippet`] — enough to carry the topic, small
/// enough that a long first reply doesn't bloat a title call.
const SNIPPET_CHARS: usize = 400;

/// The text a title is generated from: the opening exchange, not just the
/// user's first words — call sessions often open with a bare "hey boss", and
/// only the assistant's reply carries the actual topic.
pub(crate) fn exchange_snippet(user: &str, assistant: &str) -> String {
    format!(
        "User: {}\nAssistant: {}",
        truncate_chars(user.trim(), SNIPPET_CHARS),
        truncate_chars(assistant.trim(), SNIPPET_CHARS),
    )
}

fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Reasoning models sometimes inline their thinking in the reply; a title must
/// never be scraped from it. Removes complete `<think>…</think>` spans and,
/// when the close tag is missing (thinking truncated by max_tokens), everything
/// from `<think>` on — an empty remainder correctly yields "no title".
fn strip_think(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

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

    /// One title-gen model call: turns `text` into a clean, short title, or
    /// `None` on a provider failure or an empty reply. The single source of the
    /// titling prompt + model call — shared by first-turn titling
    /// ([`Self::generate_title`]) and the backfill op ([`Self::backfill_titles`])
    /// so both name sessions identically.
    pub(crate) async fn title_for(&self, text: &str) -> Option<String> {
        let provider = self.provider();
        let mut request = ChatRequest::new(TITLE_SYSTEM, vec![ChatMessage::user(text)]);
        // Room for a reasoning main model to finish thinking AND emit the
        // title — 24 used to truncate mid-<think>, yielding garbage or nothing.
        request.max_tokens = Some(512);
        let raw = match provider.complete(&request).await {
            Ok(resp) => resp.message.content.unwrap_or_default(),
            Err(error) => {
                tracing::warn!(%error, "title generation call failed");
                return None;
            }
        };
        let title = clean_title(&strip_think(&raw));
        if title.is_empty() { None } else { Some(title) }
    }

    /// Generate + store a title for `session_id`'s first turn, then emit
    /// `session.titled {session_id, title}`. Re-checks the untitled invariant
    /// under fresh state (guards against a race with a concurrent rename). Any
    /// failure is logged and swallowed — never surfaced to the turn.
    pub async fn generate_title(&self, session_id: SessionId, first_user_text: String) {
        if self.session_has_title(&session_id) {
            return;
        }
        let Some(title) = self.title_for(&first_user_text).await else {
            return;
        };
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
#[path = "titling_tests.rs"]
mod tests;
