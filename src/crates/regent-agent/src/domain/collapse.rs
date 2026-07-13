//! Mid-tier collapse (gap C3), between result-pruning and compaction: stale
//! tool EXCHANGES lose their other fat half — the assistant's tool-call
//! ARGUMENTS (file contents in `write_file`, whole patches in `apply_patch`),
//! which result-pruning never touches. Ids, names, roles, and message order
//! are preserved, so the transcript stays provider-legal. Same discipline as
//! `compression::prune_tool_results`: staleness by trailing user-turns, an
//! absolutely protected tail, idempotent stubs, and the batch floor so each
//! collapse pays for the cache reset it forces.

use crate::domain::compression::PRUNE_BATCH_MIN_TOKENS;
use regent_kernel::{ChatMessage, Role};

/// Replacement arguments for a collapsed stale tool call. Valid JSON — the
/// transcript and every provider request stay well-formed.
pub const COLLAPSED_ARGS_STUB: &str = r#"{"collapsed":"arguments elided from a stale exchange"}"#;

/// Returns `Some(new_messages)` with stale tool-call arguments stubbed when
/// the reclaimable volume clears the batch floor, else `None`.
#[must_use]
pub fn collapse_tool_exchanges(
    messages: &[ChatMessage],
    collapse_after_turns: usize,
    protect_last_n: usize,
) -> Option<Vec<ChatMessage>> {
    let len = messages.len();
    let protected_from = len.saturating_sub(protect_last_n);
    // How many user turns follow each index — the staleness measure.
    let mut user_after = vec![0usize; len];
    let mut running = 0usize;
    for i in (0..len).rev() {
        user_after[i] = running;
        if messages[i].role == Role::User {
            running += 1;
        }
    }

    let mut reclaimable_chars = 0usize;
    let mut targets = Vec::new();
    for (i, message) in messages.iter().enumerate() {
        if message.role != Role::Assistant || message.tool_calls.is_empty() || i >= protected_from {
            continue;
        }
        if user_after[i] < collapse_after_turns {
            continue;
        }
        let saved: usize = message
            .tool_calls
            .iter()
            .filter(|c| c.arguments != COLLAPSED_ARGS_STUB)
            .map(|c| c.arguments.len().saturating_sub(COLLAPSED_ARGS_STUB.len()))
            .sum();
        if saved == 0 {
            continue;
        }
        reclaimable_chars += saved;
        targets.push(i);
    }

    if targets.is_empty() {
        return None;
    }
    let reclaimable_tokens = u32::try_from(reclaimable_chars / 4).unwrap_or(u32::MAX);
    if reclaimable_tokens < PRUNE_BATCH_MIN_TOKENS {
        return None;
    }

    let mut out = messages.to_vec();
    for i in targets {
        for call in &mut out[i].tool_calls {
            if call.arguments.len() > COLLAPSED_ARGS_STUB.len() {
                call.arguments = COLLAPSED_ARGS_STUB.to_owned();
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::compression::estimate_tokens;
    use regent_kernel::{ToolCall, Transcript};

    /// A write_file-heavy session: the bulk sits in tool-call ARGUMENTS,
    /// which result-pruning never reclaims.
    fn fat_args_session(turns: usize, args_chars: usize) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        for t in 0..turns {
            messages.push(ChatMessage::user(format!("q{t}")));
            let id = format!("c{t}");
            messages.push(ChatMessage::assistant(
                None,
                vec![ToolCall {
                    id: id.clone(),
                    name: "write_file".into(),
                    arguments: format!("{{\"content\":\"{}\"}}", "y".repeat(args_chars)),
                }],
            ));
            messages.push(ChatMessage::tool_result(id, "write_file", "{\"ok\":true}"));
            messages.push(ChatMessage::assistant(Some(format!("done{t}")), vec![]));
        }
        messages
    }

    #[test]
    fn collapse_stubs_stale_arguments_and_stays_legal() {
        let messages = fat_args_session(30, 4000);
        let before = estimate_tokens("sys", &messages);
        let collapsed = collapse_tool_exchanges(&messages, 10, 8).expect("collapse should fire");
        let after = estimate_tokens("sys", &collapsed);
        assert!(after * 100 <= before * 60, "collapsed {after} vs {before}");

        let mut transcript = Transcript::new();
        for message in &collapsed {
            transcript.push(message.clone()).expect("stays legal");
        }
        assert!(!transcript.pending_tool_calls());
        // Some stale exchange is stubbed; the protected tail keeps its args.
        let protected_from = messages.len() - 8;
        assert!(collapsed.iter().enumerate().any(|(i, m)| {
            i < protected_from
                && m.tool_calls
                    .iter()
                    .any(|c| c.arguments == COLLAPSED_ARGS_STUB)
        }));
        for (i, m) in collapsed.iter().enumerate() {
            if i >= protected_from {
                for c in &m.tool_calls {
                    assert_ne!(c.arguments, COLLAPSED_ARGS_STUB, "tail untouched");
                }
            }
        }
        // Idempotent: a second pass reclaims nothing new below the floor.
        assert!(collapse_tool_exchanges(&collapsed, 10, 8).is_none());
    }

    #[test]
    fn collapse_skips_below_batch_threshold() {
        let messages = fat_args_session(20, 30);
        assert!(collapse_tool_exchanges(&messages, 5, 4).is_none());
    }
}
