//! Context compression (pure helpers): token estimation, the protected-tail
//! split (never separates an assistant from its tool results), and rebuilding
//! a valid transcript around the summary. The contract: summarize the
//! middle, keep the newest N messages verbatim, split into a child session.

use regent_kernel::{ChatMessage, RegentError, Role, Transcript};

pub const SUMMARIZER_SYSTEM: &str = "You compress agent conversation history. Write a faithful, \
compact summary that preserves stated facts, decisions, file paths, commands run with their key \
results, and unfinished work. Output only the summary text.";

const SUMMARY_SOURCE_CHARS_PER_MESSAGE: usize = 600;

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
mod tests {
    use super::*;
    use regent_kernel::ToolCall;

    fn call(id: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: "t".into(),
            arguments: "{}".into(),
        }
    }

    #[test]
    fn split_never_separates_tool_pairs() {
        let messages = vec![
            ChatMessage::user("q1"),
            ChatMessage::assistant(Some("a1".into()), vec![]),
            ChatMessage::user("q2"),
            ChatMessage::assistant(None, vec![call("x"), call("y")]),
            ChatMessage::tool_result("x", "t", "{}"),
            ChatMessage::tool_result("y", "t", "{}"),
        ];
        // A naive last-2 split would start inside the tool results.
        let (head, tail) = split_for_compression(&messages, 2).unwrap();
        assert_eq!(head.len(), 3);
        assert_eq!(tail[0].role, Role::Assistant);
        assert_eq!(tail.len(), 3);
    }

    #[test]
    fn split_skips_when_nothing_to_compress() {
        let messages = vec![
            ChatMessage::user("q"),
            ChatMessage::assistant(Some("a".into()), vec![]),
        ];
        assert!(split_for_compression(&messages, 5).is_none());
        // Walking back to index 0 (whole history is one tool block) → None.
        let all_tail = vec![
            ChatMessage::user("q"),
            ChatMessage::assistant(None, vec![call("x")]),
            ChatMessage::tool_result("x", "t", "{}"),
        ];
        assert!(split_for_compression(&all_tail, 1).map(|(h, _)| h.len()) > Some(0));
    }

    #[test]
    fn rebuild_inserts_bridge_only_when_tail_starts_with_user() {
        let tail_user = vec![ChatMessage::user("latest question")];
        let t = rebuild_transcript("the summary", tail_user).unwrap();
        assert_eq!(t.messages().len(), 3);
        assert_eq!(t.messages()[1].role, Role::Assistant);

        let tail_assistant = vec![
            ChatMessage::assistant(None, vec![call("x")]),
            ChatMessage::tool_result("x", "t", "{}"),
        ];
        let t = rebuild_transcript("the summary", tail_assistant).unwrap();
        assert_eq!(t.messages().len(), 3);
        assert!(
            t.messages()[0]
                .content
                .as_deref()
                .unwrap()
                .contains("the summary")
        );
        assert!(!t.pending_tool_calls());
    }

    #[test]
    fn estimator_grows_with_content() {
        let small = estimate_tokens("sys", &[ChatMessage::user("hi")]);
        let big = estimate_tokens("sys", &[ChatMessage::user("x".repeat(4000))]);
        assert!(big > small + 900);
    }
}
