use crate::types::error::RegentError;
use crate::types::message::{ChatMessage, Role};
use std::collections::HashSet;

/// Conversation history that can only be appended to in provider-legal
/// order (the Hermes alternation invariant, enforced by construction):
///
/// - first message is `user`
/// - never two `user` or two `assistant` messages in a row
/// - `tool` messages only while an assistant's tool calls are pending, and
///   each must answer exactly one pending `tool_call_id`
/// - no `user`/`assistant` message while tool calls are still unanswered
#[derive(Debug, Default, Clone)]
pub struct Transcript {
    messages: Vec<ChatMessage>,
    pending_tool_ids: HashSet<String>,
}

impl Transcript {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    #[must_use]
    pub fn pending_tool_calls(&self) -> bool {
        !self.pending_tool_ids.is_empty()
    }

    fn last_role(&self) -> Option<Role> {
        self.messages.last().map(|m| m.role)
    }

    pub fn push(&mut self, message: ChatMessage) -> Result<(), RegentError> {
        match message.role {
            Role::User => self.check_user()?,
            Role::Assistant => self.check_assistant()?,
            Role::Tool => self.check_tool(&message)?,
        }
        if message.role == Role::Assistant {
            self.pending_tool_ids = message.tool_calls.iter().map(|c| c.id.clone()).collect();
        } else if message.role == Role::Tool
            && let Some(id) = &message.tool_call_id
        {
            self.pending_tool_ids.remove(id);
        }
        self.messages.push(message);
        Ok(())
    }

    /// Recovery: drop a trailing user message left by a failed/interrupted turn
    /// (no assistant reply followed), so the next turn can push a fresh user
    /// message legally. No-op unless the last message is a user with no tool
    /// calls pending. Returns whether a message was removed.
    pub fn drop_trailing_user(&mut self) -> bool {
        if self.pending_tool_calls() || self.last_role() != Some(Role::User) {
            return false;
        }
        self.messages.pop();
        true
    }

    fn check_user(&self) -> Result<(), RegentError> {
        if self.pending_tool_calls() {
            return Err(RegentError::Transcript(
                "user message while tool calls are pending".into(),
            ));
        }
        if self.last_role() == Some(Role::User) {
            return Err(RegentError::Transcript("two user messages in a row".into()));
        }
        Ok(())
    }

    fn check_assistant(&self) -> Result<(), RegentError> {
        if self.is_empty() {
            return Err(RegentError::Transcript(
                "assistant message cannot open a conversation".into(),
            ));
        }
        if self.pending_tool_calls() {
            return Err(RegentError::Transcript(
                "assistant message while tool calls are pending".into(),
            ));
        }
        if self.last_role() == Some(Role::Assistant) {
            return Err(RegentError::Transcript(
                "two assistant messages in a row".into(),
            ));
        }
        Ok(())
    }

    fn check_tool(&self, message: &ChatMessage) -> Result<(), RegentError> {
        let id = message.tool_call_id.as_deref().ok_or_else(|| {
            RegentError::Transcript("tool message without tool_call_id".into())
        })?;
        if !self.pending_tool_ids.contains(id) {
            return Err(RegentError::Transcript(format!(
                "tool result for unknown/answered call id '{id}'"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::message::ToolCall;

    fn call(id: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: "echo".into(),
            arguments: "{}".into(),
        }
    }

    #[test]
    fn legal_tool_round_trip() {
        let mut t = Transcript::new();
        t.push(ChatMessage::user("hi")).unwrap();
        t.push(ChatMessage::assistant(None, vec![call("a"), call("b")]))
            .unwrap();
        assert!(t.pending_tool_calls());
        t.push(ChatMessage::tool_result("b", "echo", "{}")).unwrap();
        t.push(ChatMessage::tool_result("a", "echo", "{}")).unwrap();
        assert!(!t.pending_tool_calls());
        t.push(ChatMessage::assistant(Some("done".into()), vec![]))
            .unwrap();
        t.push(ChatMessage::user("thanks")).unwrap();
        assert_eq!(t.messages().len(), 6);
    }

    #[test]
    fn rejects_alternation_violations() {
        let mut t = Transcript::new();
        assert!(t.push(ChatMessage::assistant(Some("x".into()), vec![])).is_err());
        t.push(ChatMessage::user("hi")).unwrap();
        assert!(t.push(ChatMessage::user("again")).is_err());
        t.push(ChatMessage::assistant(Some("ok".into()), vec![])).unwrap();
        assert!(t.push(ChatMessage::assistant(Some("ok2".into()), vec![])).is_err());
    }

    #[test]
    fn drop_trailing_user_recovers_a_failed_turn() {
        let mut t = Transcript::new();
        t.push(ChatMessage::user("hi")).unwrap();
        // A failed turn left a dangling user; recovery removes it so the next
        // user message is legal again.
        assert!(t.drop_trailing_user());
        assert!(t.is_empty());
        t.push(ChatMessage::user("retry")).unwrap();

        // No-op when the last message isn't a user…
        t.push(ChatMessage::assistant(Some("ok".into()), vec![])).unwrap();
        assert!(!t.drop_trailing_user());
        assert_eq!(t.messages().len(), 2);

        // …and a no-op (won't strip a user) while tool calls are pending.
        let mut p = Transcript::new();
        p.push(ChatMessage::user("hi")).unwrap();
        p.push(ChatMessage::assistant(None, vec![call("a")])).unwrap();
        assert!(!p.drop_trailing_user());
    }

    #[test]
    fn rejects_messages_while_tools_pending_and_bad_ids() {
        let mut t = Transcript::new();
        t.push(ChatMessage::user("hi")).unwrap();
        t.push(ChatMessage::assistant(None, vec![call("a")])).unwrap();
        assert!(t.push(ChatMessage::user("nope")).is_err());
        assert!(t.push(ChatMessage::assistant(Some("nope".into()), vec![])).is_err());
        assert!(t.push(ChatMessage::tool_result("zz", "echo", "{}")).is_err());
        t.push(ChatMessage::tool_result("a", "echo", "{}")).unwrap();
        // answering the same id twice is rejected
        assert!(t.push(ChatMessage::tool_result("a", "echo", "{}")).is_err());
    }
}
