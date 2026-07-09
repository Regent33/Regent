//! Transcript translation: the internal OpenAI-style message list → Anthropic's
//! alternating block-structured turns. Every run of user/tool messages becomes
//! one `user` turn (order preserved); each assistant message becomes one
//! `assistant` turn with an optional replayed thinking block, text, and
//! `tool_use` blocks.

use regent_kernel::{ChatMessage, Role};
use serde_json::{Value, json};

pub(super) fn build_messages(messages: &[ChatMessage]) -> Value {
    let mut out: Vec<Value> = Vec::new();
    let mut user_blocks: Vec<Value> = Vec::new();

    let flush_user = |blocks: &mut Vec<Value>, out: &mut Vec<Value>| {
        if !blocks.is_empty() {
            out.push(json!({"role": "user", "content": std::mem::take(blocks)}));
        }
    };

    for message in messages {
        match message.role {
            Role::User => {
                if let Some(text) = &message.content {
                    user_blocks.push(json!({"type": "text", "text": text}));
                }
            }
            Role::Tool => {
                user_blocks.push(json!({
                    "type": "tool_result",
                    "tool_use_id": message.tool_call_id.as_deref().unwrap_or(""),
                    "content": message.content.as_deref().unwrap_or(""),
                }));
            }
            Role::Assistant => {
                flush_user(&mut user_blocks, &mut out);
                let mut blocks: Vec<Value> = Vec::new();
                // Replay the thinking block verbatim (text + signature) as the
                // FIRST block — required for multi-turn tool use with extended
                // thinking. Only when both are present (a signature is the
                // proof the block is genuine; unsigned reasoning is dropped).
                if let (Some(thinking), Some(signature)) =
                    (&message.reasoning, &message.thinking_signature)
                {
                    blocks.push(json!({
                        "type": "thinking",
                        "thinking": thinking,
                        "signature": signature,
                    }));
                }
                if let Some(text) = &message.content
                    && !text.is_empty()
                {
                    blocks.push(json!({"type": "text", "text": text}));
                }
                for call in &message.tool_calls {
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.name,
                        "input": parse_arguments(&call.arguments),
                    }));
                }
                // An assistant turn must carry at least one block.
                if blocks.is_empty() {
                    blocks.push(json!({"type": "text", "text": ""}));
                }
                out.push(json!({"role": "assistant", "content": blocks}));
            }
        }
    }
    flush_user(&mut user_blocks, &mut out);
    Value::Array(out)
}

/// Tool-call arguments are stored as a JSON string internally; Anthropic
/// wants the parsed object. Fall back to an empty object on malformed input.
fn parse_arguments(arguments: &str) -> Value {
    serde_json::from_str(arguments).unwrap_or_else(|_| json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_kernel::ToolCall;

    #[test]
    fn tool_calls_become_tool_use_blocks_with_parsed_input() {
        let assistant = ChatMessage::assistant(
            Some("calling".to_owned()),
            vec![ToolCall {
                id: "toolu_1".to_owned(),
                name: "read_file".to_owned(),
                arguments: r#"{"path":"/tmp/x"}"#.to_owned(),
            }],
        );
        let out = build_messages(&[assistant]);
        assert_eq!(out[0]["content"][0]["type"], "text");
        assert_eq!(out[0]["content"][1]["type"], "tool_use");
        assert_eq!(out[0]["content"][1]["input"]["path"], "/tmp/x");
    }

    #[test]
    fn tool_results_collapse_into_one_user_turn() {
        let messages = vec![
            ChatMessage::tool_result("toolu_1", "read_file", "contents-a"),
            ChatMessage::tool_result("toolu_2", "read_file", "contents-b"),
        ];
        let out = build_messages(&messages);
        let msgs = out.as_array().unwrap();
        assert_eq!(msgs.len(), 1, "two tool results must share one user turn");
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"][0]["tool_use_id"], "toolu_1");
        assert_eq!(msgs[0]["content"][1]["tool_use_id"], "toolu_2");
    }

    #[test]
    fn roles_alternate_after_translation() {
        let messages = vec![
            ChatMessage::user("do it"),
            ChatMessage::assistant(
                None,
                vec![ToolCall {
                    id: "t1".into(),
                    name: "x".into(),
                    arguments: "{}".into(),
                }],
            ),
            ChatMessage::tool_result("t1", "x", "done"),
        ];
        let out = build_messages(&messages);
        let roles: Vec<&str> = out
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, vec!["user", "assistant", "user"]);
    }

    #[test]
    fn signed_thinking_block_is_replayed_first() {
        let mut assistant = ChatMessage::assistant(Some("answer".into()), vec![]);
        assistant.reasoning = Some("because".into());
        assistant.thinking_signature = Some("sig-1".into());
        let out = build_messages(&[assistant]);
        assert_eq!(out[0]["content"][0]["type"], "thinking");
        assert_eq!(out[0]["content"][0]["signature"], "sig-1");
        assert_eq!(out[0]["content"][1]["type"], "text");
    }

    #[test]
    fn unsigned_reasoning_is_not_replayed() {
        // Reasoning without a signature can't be re-sent (it would fail
        // validation), so it's dropped rather than replayed.
        let mut assistant = ChatMessage::assistant(Some("answer".into()), vec![]);
        assistant.reasoning = Some("because".into());
        let out = build_messages(&[assistant]);
        let blocks = out[0]["content"].as_array().unwrap();
        assert!(
            blocks.iter().all(|b| b["type"] != "thinking"),
            "no signature → no replay"
        );
    }
}
