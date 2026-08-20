//! `react_to_message` — put an emoji reaction on a chat message. The chat-native
//! way to say "seen", "done", or "yes" without adding another message to the
//! thread.
//!
//! Gateway-only: it is registered exactly where `send_message` gets a real
//! platform sink, so the CLI and desktop catalogs (which have no such thing as a
//! message reaction) pay nothing for it.

use crate::ToolCatalog;
use crate::domain::contracts::{ReactionSink, ToolExecutor};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};
use std::sync::Arc;

/// Longest accepted emoji in `char`s. A single reaction can legitimately be
/// several code points — a ZWJ sequence (👨‍💻), a skin-tone modifier, or a
/// variation selector (❤️) — but nothing sane is longer than this.
const MAX_EMOJI_CHARS: usize = 8;

#[must_use]
pub fn definition(targets: &[String]) -> ToolDefinition {
    let where_to = if targets.is_empty() {
        String::new()
    } else {
        format!(" Reacts in {}.", targets.join(", "))
    };
    ToolDefinition {
        name: "react_to_message".into(),
        description: format!(
            "React to the user's message with a single emoji — to confirm you have seen or \
             done something, to answer a yes/no, or because they asked you to react. \
             Cheaper and less noisy than sending a message.{where_to}"
        ),
        parameters: json!({
            "type": "object",
            "properties": {
                "emoji": {
                    "type": "string",
                    "description": "One emoji, e.g. 👍 or 🎉. Platforms accept a limited set; \
                         an unsupported one is mapped to the closest allowed reaction."
                },
                "message_id": {
                    "type": "string",
                    "description": "Optional. Omit to react to the message that started this \
                         turn, which is almost always what you want."
                }
            },
            "required": ["emoji"]
        }),
        toolset: "core".into(),
    }
}

struct ReactTool(Arc<dyn ReactionSink>);

#[async_trait]
impl ToolExecutor for ReactTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(emoji) = args.get("emoji").and_then(Value::as_str) else {
            return Ok(tool_error_json("react_to_message needs 'emoji' (a string)"));
        };
        if let Err(message) = validate_emoji(emoji) {
            return Ok(tool_error_json(&message));
        }
        let message_id = args.get("message_id").and_then(Value::as_str);
        match self.0.react(message_id, emoji).await {
            Ok(()) => Ok(tool_result_json(json!({"reacted": emoji}))),
            Err(error) => Ok(tool_error_json(error.to_string())),
        }
    }
}

/// Rejects anything that is not plausibly a single emoji. This is a **trust
/// boundary**, not cosmetics: the value reaches a third-party URL path on
/// Discord and a validated enum on Telegram, so a newline, a slash, or a
/// 200-character string must never be built into a request.
///
/// # Errors
/// Returns a message phrased for the model when the value is not usable.
pub fn validate_emoji(emoji: &str) -> Result<(), String> {
    if emoji.is_empty() {
        return Err("react_to_message needs a non-empty emoji".to_owned());
    }
    let count = emoji.chars().count();
    if count > MAX_EMOJI_CHARS {
        return Err(format!(
            "{emoji:?} is not a single emoji ({count} characters) — react with exactly one"
        ));
    }
    if emoji
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c.is_ascii())
    {
        return Err(format!(
            "{emoji:?} is not an emoji — pass the character itself, e.g. 👍"
        ));
    }
    Ok(())
}

/// Registers `react_to_message` against a live platform sink. Surfaces with no
/// reaction concept simply never call this, so they carry no schema cost.
pub fn register_reaction_tool(
    catalog: &mut ToolCatalog,
    sink: Arc<dyn ReactionSink>,
) -> Result<(), RegentError> {
    let targets = sink.targets();
    catalog.register(definition(&targets), Arc::new(ReactTool(sink)))
}

#[cfg(test)]
#[path = "react_tests.rs"]
mod tests;
