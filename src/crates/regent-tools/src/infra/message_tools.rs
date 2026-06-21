//! `send_message` — proactive outbound delivery. The agent names a target
//! (a connected channel) and the configured [`DeliverySink`] delivers it. The
//! tool never touches a platform SDK; the surface owns transport.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::{DeliverySink, ToolExecutor};
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::sync::Arc;

/// Registers `send_message`, wired to deliver through `sink`.
pub fn register_message_tool(
    catalog: &mut ToolCatalog,
    sink: Arc<dyn DeliverySink>,
) -> Result<(), RegentError> {
    let definition = send_message_definition(&sink.targets());
    catalog.register(definition, Arc::new(SendMessageTool { sink }))
}

fn send_message_definition(targets: &[String]) -> ToolDefinition {
    let where_to = if targets.is_empty() {
        "the home channel".to_owned()
    } else {
        targets.join(", ")
    };
    ToolDefinition {
        name: "send_message".into(),
        description: format!(
            "Proactively deliver a message to the user on a connected channel. \
             Available targets: {where_to}. Omit 'target' for the home channel. \
             This sends to a real platform — use only when asked to notify or message someone."
        ),
        parameters: json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "The message to deliver."},
                "target": {"type": "string", "description": "Channel to deliver to; omit for home."}
            },
            "required": ["text"]
        }),
        toolset: "delivery".into(),
    }
}

struct SendMessageTool {
    sink: Arc<dyn DeliverySink>,
}

#[async_trait]
impl ToolExecutor for SendMessageTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(text) = args.get("text").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: text"));
        };
        if text.trim().is_empty() {
            return Ok(tool_error_json("message text is empty"));
        }
        let target = args.get("target").and_then(Value::as_str).unwrap_or("");
        match self.sink.deliver(target, text).await {
            Ok(()) => {
                let to = if target.is_empty() { "home" } else { target };
                Ok(json!({"success": true, "delivered_to": to}).to_string())
            }
            Err(error) => Ok(tool_error_json(error.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::NoDelivery;
    use std::sync::Mutex;

    /// Records what it was asked to deliver.
    #[derive(Default)]
    struct StubSink {
        sent: Mutex<Vec<(String, String)>>,
    }
    #[async_trait]
    impl DeliverySink for StubSink {
        async fn deliver(&self, target: &str, text: &str) -> Result<(), RegentError> {
            self.sent
                .lock()
                .unwrap()
                .push((target.to_owned(), text.to_owned()));
            Ok(())
        }
        fn targets(&self) -> Vec<String> {
            vec!["telegram:home".to_owned()]
        }
    }

    fn ctx() -> ToolContext {
        ToolContext::new(
            std::path::PathBuf::from("."),
            Arc::new(crate::domain::contracts::DenyAll),
        )
    }

    #[tokio::test]
    async fn delivers_text_to_the_named_target() {
        let sink = Arc::new(StubSink::default());
        let tool = SendMessageTool {
            sink: Arc::clone(&sink) as Arc<dyn DeliverySink>,
        };
        let out = tool
            .execute(
                json!({"text": "build is green", "target": "telegram:home"}),
                &ctx(),
            )
            .await
            .unwrap();
        assert!(out.contains("\"success\":true"));
        assert!(out.contains("telegram:home"));
        let sent = sink.sent.lock().unwrap();
        assert_eq!(
            sent.as_slice(),
            &[("telegram:home".to_owned(), "build is green".to_owned())]
        );
    }

    #[tokio::test]
    async fn missing_or_empty_text_is_a_tool_error_not_a_send() {
        let sink = Arc::new(StubSink::default());
        let tool = SendMessageTool {
            sink: Arc::clone(&sink) as Arc<dyn DeliverySink>,
        };
        let out = tool.execute(json!({"target": "x"}), &ctx()).await.unwrap();
        assert!(out.contains("error"));
        assert!(sink.sent.lock().unwrap().is_empty(), "nothing was sent");
    }

    #[tokio::test]
    async fn no_delivery_sink_declines_cleanly() {
        let tool = SendMessageTool {
            sink: Arc::new(NoDelivery),
        };
        let out = tool.execute(json!({"text": "hi"}), &ctx()).await.unwrap();
        assert!(out.contains("error"));
        assert!(out.contains("no delivery channels"));
    }

    #[test]
    fn definition_lists_targets_for_the_model() {
        let def = send_message_definition(&["telegram:home".to_owned(), "discord:ops".to_owned()]);
        assert!(def.description.contains("telegram:home"));
        assert!(def.description.contains("discord:ops"));
    }
}
