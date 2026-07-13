//! Context compression (pure helpers): token estimation, the protected-tail
//! split (never separates an assistant from its tool results), and rebuilding
//! a valid transcript around the summary. The contract: summarize the
//! middle, keep the newest N messages verbatim, split into a child session.

use regent_kernel::{ChatMessage, RegentError, Role, Transcript};

pub const SUMMARIZER_SYSTEM: &str = "You compress agent conversation history. Write a faithful, \
compact summary that preserves stated facts, decisions, file paths, commands run with their key \
results, and unfinished work. Output only the summary text.";

const SUMMARY_SOURCE_CHARS_PER_MESSAGE: usize = 600;

/// Replacement content for a pruned tool result (SPL §3.8). Tool results are
/// re-fetchable — the model can re-read the file or re-run the search — so the
/// stub loses no unrecoverable information.
pub const PRUNED_STUB: &str = "[result pruned — re-run the tool if needed]";

/// Batch floor: prune only when at least this many tokens (chars/4) would be
/// reclaimed. Each prune mutates Tier-2 history and forces a cache reset, so a
/// prune that reclaims scraps would cost more (a busted cache) than it saves —
/// death by a thousand cache-busts. Wait until the reclaim pays for the reset.
pub const PRUNE_BATCH_MIN_TOKENS: u32 = 2_000;

/// Tool-result pruning (SPL §3.8, the history-side lever). Replaces the content
/// of stale tool RESULT messages with [`PRUNED_STUB`], preserving every message's
/// role and `tool_call_id` so the transcript stays provider-legal (dangling tool
/// calls are rejected). Rules, all enforced here:
///
/// - Only `Role::Tool` messages are ever touched — user/assistant are immune.
/// - A result is stale once `prune_after_turns` user messages follow it.
/// - The newest `protect_last_n` messages are protected absolutely (mirrors
///   compression), regardless of age.
/// - Batched: returns `None` (prune nothing) unless the reclaimable volume
///   clears [`PRUNE_BATCH_MIN_TOKENS`], so each prune pays for its cache reset.
///
/// Returns `Some(new_messages)` with stubs applied when pruning fires, else
/// `None`. Idempotent: an already-stubbed result reclaims nothing and is skipped.
#[must_use]
pub fn prune_tool_results(
    messages: &[ChatMessage],
    prune_after_turns: usize,
    protect_last_n: usize,
) -> Option<Vec<ChatMessage>> {
    let len = messages.len();
    let protected_from = len.saturating_sub(protect_last_n);

    // Suffix count: how many user messages appear strictly after each index —
    // i.e. how many turns old a result at that index is.
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
        if message.role != Role::Tool || i >= protected_from {
            continue;
        }
        if user_after[i] < prune_after_turns {
            continue;
        }
        let current = message.content.as_deref().unwrap_or("");
        if current == PRUNED_STUB {
            continue; // already pruned — idempotent, reclaims nothing
        }
        let saved = current.len().saturating_sub(PRUNED_STUB.len());
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
        return None; // below the batch floor — not worth the cache reset
    }

    let mut out = messages.to_vec();
    for i in targets {
        out[i].content = Some(PRUNED_STUB.to_owned());
    }
    Some(out)
}

/// Rough prompt-size estimate (chars/4) over system prompt + transcript.
#[must_use]
pub fn estimate_tokens(system: &str, messages: &[ChatMessage]) -> u32 {
    let mut chars = system.len();
    for message in messages {
        chars += message.content.as_deref().map_or(0, str::len);
        for call in &message.tool_calls {
            chars += call.name.len() + call.arguments.len();
        }
        chars += 16; // role + framing overhead
    }
    u32::try_from(chars / 4).unwrap_or(u32::MAX)
}

/// Splits history into (head-to-summarize, tail-kept-verbatim). The tail
/// boundary walks backwards over tool results so an assistant message and
/// its results are never separated. Returns None when there is nothing
/// meaningful to compress.
#[must_use]
pub fn split_for_compression(
    messages: &[ChatMessage],
    protect_last_n: usize,
) -> Option<(Vec<ChatMessage>, Vec<ChatMessage>)> {
    if messages.len() <= protect_last_n {
        return None;
    }
    let mut start = messages.len() - protect_last_n;
    while start > 0 && messages[start].role == Role::Tool {
        start -= 1;
    }
    if start == 0 {
        return None;
    }
    Some((messages[..start].to_vec(), messages[start..].to_vec()))
}

/// Renders the head as role-labeled text for the summarizer model.
#[must_use]
pub fn render_for_summary(head: &[ChatMessage]) -> String {
    let mut out = String::from("Conversation to summarize:\n\n");
    for message in head {
        let body = match (&message.content, message.tool_calls.is_empty()) {
            (Some(content), true) => content.clone(),
            (content, false) => {
                let calls: Vec<String> = message
                    .tool_calls
                    .iter()
                    .map(|c| format!("{}({})", c.name, c.arguments))
                    .collect();
                format!(
                    "{} [tool calls: {}]",
                    content.clone().unwrap_or_default(),
                    calls.join(", ")
                )
            }
            (None, true) => String::new(),
        };
        out.push_str(&format!("{}: {}\n", message.role.as_str(), cap(&body)));
    }
    out
}

fn cap(text: &str) -> String {
    if text.chars().count() <= SUMMARY_SOURCE_CHARS_PER_MESSAGE {
        return text.to_owned();
    }
    let kept: String = text
        .chars()
        .take(SUMMARY_SOURCE_CHARS_PER_MESSAGE)
        .collect();
    format!("{kept}…")
}

/// Builds the compressed transcript: summary as the opening user message,
/// an assistant bridge when the tail would otherwise break alternation,
/// then the verbatim tail — all re-validated by `Transcript`.
pub fn rebuild_transcript(
    summary: &str,
    tail: Vec<ChatMessage>,
) -> Result<Transcript, RegentError> {
    let mut transcript = Transcript::new();
    transcript.push(ChatMessage::user(format!(
        "[CONTEXT SUMMARY — earlier conversation was compressed]\n{summary}"
    )))?;
    if tail.first().map(|m| m.role) == Some(Role::User) {
        transcript.push(ChatMessage::assistant(
            Some("Understood — continuing from the summary.".to_owned()),
            vec![],
        ))?;
    }
    for message in tail {
        transcript.push(message)?;
    }
    Ok(transcript)
}

#[cfg(test)]
#[path = "compression_tests.rs"]
mod tests;
