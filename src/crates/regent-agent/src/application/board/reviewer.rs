//! The agent-backed [`Reviewer`]: judges a task's submitted work in a fresh
//! agent (review source) and maps its reply to a verdict. The verdict marker
//! is parsed deterministically; anything ambiguous is treated as a rejection,
//! so the `agent` policy never auto-completes work on an unclear review.

use super::{ReviewVerdict, Reviewer};
use crate::application::agent::Agent;
use crate::domain::config::AgentConfig;
use async_trait::async_trait;
use regent_kernel::RegentError;
use regent_providers::ChatProvider;
use regent_store::{KanbanTaskRow, Store};
use regent_tools::{ToolCatalog, ToolContext};
use std::sync::Arc;

const REVIEW_PROMPT: &str = "You are a strict reviewer on a work board. You are given a \
task and the worker's submitted result. Decide whether the result genuinely satisfies the \
task. Reply with EXACTLY one line: `APPROVE` if it does, or `REJECT: <short reason>` if it \
does not. Do not add anything else.";

pub struct AgentReviewer {
    provider: Arc<dyn ChatProvider>,
    catalog: Arc<ToolCatalog>,
    store: Arc<Store>,
    tool_context: ToolContext,
    max_iterations: u32,
}

impl AgentReviewer {
    #[must_use]
    pub fn new(
        provider: Arc<dyn ChatProvider>,
        catalog: Arc<ToolCatalog>,
        store: Arc<Store>,
        tool_context: ToolContext,
    ) -> Self {
        Self {
            provider,
            catalog,
            store,
            tool_context,
            max_iterations: 6,
        }
    }
}

#[async_trait]
impl Reviewer for AgentReviewer {
    async fn review(&self, task: &KanbanTaskRow, work: &str) -> Result<ReviewVerdict, RegentError> {
        let config = AgentConfig {
            source: "review".to_owned(),
            max_iterations: self.max_iterations,
            ..AgentConfig::default()
        };
        let mut agent = Agent::new(
            Arc::clone(&self.provider),
            Arc::clone(&self.catalog),
            Arc::clone(&self.store),
            self.tool_context.clone(),
            REVIEW_PROMPT.to_owned(),
            config,
        )?;
        let prompt = format!(
            "TASK: {}\n{}\n\nWORKER RESULT:\n{work}\n\nVerdict:",
            task.title, task.description
        );
        let output = agent.run_turn(&prompt).await?;
        Ok(parse_verdict(&output))
    }
}

/// Maps the reviewer's free text to a verdict. The first line that starts
/// (case-insensitively) with `APPROVE` or `REJECT` wins; anything else is a
/// rejection — the conservative default (never auto-approve on ambiguity).
fn parse_verdict(text: &str) -> ReviewVerdict {
    for raw in text.lines() {
        let line = raw.trim();
        let bytes = line.as_bytes();
        if bytes.len() >= 7 && bytes[..7].eq_ignore_ascii_case(b"APPROVE") {
            return ReviewVerdict::Approve;
        }
        if bytes.len() >= 6 && bytes[..6].eq_ignore_ascii_case(b"REJECT") {
            let reason = line[6..].trim_start_matches([':', ' ', '-']).trim();
            let reason = if reason.is_empty() {
                "rejected without a stated reason"
            } else {
                reason
            };
            return ReviewVerdict::Reject(reason.to_owned());
        }
    }
    ReviewVerdict::Reject("reviewer returned no clear verdict".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approve_is_parsed_case_insensitively() {
        assert_eq!(parse_verdict("APPROVE"), ReviewVerdict::Approve);
        assert_eq!(parse_verdict("approve"), ReviewVerdict::Approve);
        assert_eq!(
            parse_verdict("Approved — looks good"),
            ReviewVerdict::Approve
        );
    }

    #[test]
    fn reject_carries_the_reason() {
        assert_eq!(
            parse_verdict("REJECT: missing tests"),
            ReviewVerdict::Reject("missing tests".to_owned())
        );
        assert_eq!(
            parse_verdict("reject - did not compile"),
            ReviewVerdict::Reject("did not compile".to_owned())
        );
    }

    #[test]
    fn reject_without_reason_gets_a_placeholder() {
        assert_eq!(
            parse_verdict("REJECT"),
            ReviewVerdict::Reject("rejected without a stated reason".to_owned())
        );
    }

    #[test]
    fn verdict_is_found_past_preamble_lines() {
        let text = "Let me think about this.\nThe result is incomplete.\nREJECT: no error handling";
        assert_eq!(
            parse_verdict(text),
            ReviewVerdict::Reject("no error handling".to_owned())
        );
    }

    #[test]
    fn ambiguous_output_defaults_to_reject() {
        // Never auto-approve when the reviewer gave no clear verdict.
        match parse_verdict("I'm not sure, it could go either way.") {
            ReviewVerdict::Reject(_) => {}
            other => panic!("expected reject, got {other:?}"),
        }
    }
}
