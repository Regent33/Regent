//! Model-output validation and the request-local repair/retry state: a
//! completion with no final text or tool call gets one silent retry, and a
//! textual imitation of a tool call is rejected rather than entering history.

use crate::application::agent::Agent;
use regent_kernel::{ChatMessage, RegentError, ToolDefinition};

/// Request-local correction for a model that planned in its reasoning channel
/// but emitted neither a final answer nor a tool call. This is never persisted.
pub(super) const REASONING_ONLY_REPAIR: &str = "Your previous response contained only private reasoning, \
with no final answer or tool call. Continue the original task now: call an available tool, or \
call load_tools first if the needed capability is deferred, or give the user a final answer. \
Do not output analysis or describe a tool call without actually making it.";

/// Request-local correction for providers/models that print a tool-shaped
/// string (for example `[web_search: \"query\"]`) instead of emitting a native
/// tool call. The rejected text never enters conversation history.
pub(super) const PSEUDO_TOOL_REPAIR: &str = "Your previous response printed a textual imitation of a tool \
call. That text was rejected. Continue the original task now by emitting a native structured \
tool call through the API. Do not write `[tool: arguments]` or fabricate a tool result.";

/// Request-local correction for a model that announced work and then stopped
/// without calling anything. Distinct from `REASONING_ONLY_REPAIR` because the
/// diagnosis differs: there WAS a visible answer, it just promised instead of
/// acting, and telling the model it "contained only private reasoning" would be
/// a false statement about its own last message.
pub(super) const PROMISED_WORK_REPAIR: &str = "Your previous response said you would do the work but made no \
tool call, so nothing happened. Every deferred tool is now visible. Do the work now with real tool calls — \
create the files you described. If you cannot, say plainly what is missing instead of describing work you \
did not do.";

/// Per-turn retry/repair flags. Each recovery fires at most once per turn;
/// the repair messages are request-local and never persisted.
pub(super) struct RetryState {
    pub empty_retried: bool,
    pub repair_reasoning_only: bool,
    pub pseudo_retried: bool,
    pub repair_pseudo_tool: bool,
    /// Prose that promised work and called nothing. Separate one-shot from
    /// `empty_retried`: that path needs empty content, this one needs the
    /// opposite, and a turn can plausibly hit both.
    pub promise_retried: bool,
    pub repair_promised_work: bool,
    /// A weak model can name the right deferred capability in reasoning but
    /// call a visible tool with the wrong schema instead. One failed call
    /// earns one full-catalog retry; steady-state light turns stay lean.
    pub error_recovery_attempted: bool,
}

impl RetryState {
    pub fn new() -> Self {
        Self {
            empty_retried: false,
            repair_reasoning_only: false,
            pseudo_retried: false,
            repair_pseudo_tool: false,
            promise_retried: false,
            repair_promised_work: false,
            error_recovery_attempted: false,
        }
    }
}

pub(super) enum OutputCheck {
    /// The completion was unusable — retry the model call (repair flags set).
    Retry,
    /// The completion is real output — push it and carry on.
    Proceed,
}

pub(super) fn pseudo_tool_name<'a>(
    content: &str,
    mut known: impl Iterator<Item = &'a str>,
) -> Option<String> {
    let body = content.trim_start().strip_prefix('[')?;
    let (candidate, _) = body.split_once(':')?;
    let candidate = candidate.trim();
    if candidate.is_empty()
        || !candidate
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return None;
    }
    known
        .any(|name| name == candidate)
        .then(|| candidate.to_owned())
}

impl Agent {
    /// Validates one completion. On an unusable one (pseudo tool call or no
    /// output at all) sets the repair flags — revealing deferred tools when
    /// that's the likely cause — and asks the caller to retry; the second
    /// occurrence is a provider error.
    pub(super) fn check_model_output(
        &mut self,
        assistant: &ChatMessage,
        definitions: &mut Vec<ToolDefinition>,
        retry: &mut RetryState,
    ) -> Result<OutputCheck, RegentError> {
        let pseudo_tool = assistant
            .tool_calls
            .is_empty()
            .then_some(assistant.content.as_deref())
            .flatten()
            .and_then(|content| {
                pseudo_tool_name(
                    content,
                    definitions
                        .iter()
                        .map(|definition| definition.name.as_str()),
                )
            });
        if let Some(tool) = pseudo_tool {
            if retry.pseudo_retried {
                return Err(RegentError::Provider(format!(
                    "{} emitted a textual imitation of the {tool} tool call twice; switch models or verify this provider's native tool-call support",
                    self.provider.model()
                )));
            }
            retry.pseudo_retried = true;
            retry.repair_pseudo_tool = true;
            retry.repair_reasoning_only = false;
            tracing::warn!(
                model = self.provider.model(),
                %tool,
                "model emitted a pseudo tool call in content — retrying once"
            );
            return Ok(OutputCheck::Retry);
        }
        let reasoning_only = assistant.tool_calls.is_empty()
            && assistant
                .content
                .as_deref()
                .is_none_or(|c| c.trim().is_empty())
            && assistant
                .reasoning
                .as_deref()
                .is_some_and(|r| !r.trim().is_empty());
        if assistant.tool_calls.is_empty()
            && assistant
                .content
                .as_deref()
                .is_none_or(|c| c.trim().is_empty())
        {
            // Not pushed/persisted — an empty assistant row is junk in
            // history and breaks nothing by being absent.
            if retry.empty_retried {
                return Err(RegentError::Provider(format!(
                    "{} returned an empty response or private reasoning without an answer \
                     twice — try again, or switch models if it keeps happening",
                    self.provider.model()
                )));
            }
            retry.empty_retried = true;
            retry.repair_reasoning_only = reasoning_only;
            // A reasoning-only dead-end usually means the model needs a tool
            // whose schema is deferred (hidden) and it won't call load_tools
            // itself — the "empty response … twice" bug on tool-shaped asks
            // (save a key, `regent` admin, etc.). Reveal every deferred tool
            // and rebuild the list so the retry sees them; token-efficient
            // vs. shipping the full catalog every turn.
            let revealed = self.catalog.reveal_all_deferred();
            if revealed > 0 {
                *definitions = self.catalog.definitions();
                // The Tier-0 definitions payload just changed on purpose: tag
                // the turn so the deacon rebases its stable-prefix baseline
                // instead of reporting the same cache_bust every later turn.
                self.note_cache_reset("tiering");
            }
            tracing::warn!(
                model = self.provider.model(),
                reasoning_only,
                revealed,
                "model returned no final answer or tool call — revealing deferred tools, retrying once"
            );
            return Ok(OutputCheck::Retry);
        }
        // The turn LOOKED like a success: real prose, no tool call, `outcome=ok`
        // — and nothing created. The branch above cannot see it, because it
        // requires empty content and this content is the problem.
        //
        // Gated on `revealed > 0`, which does three jobs at once: it limits the
        // net to sessions that are actually hiding capability (the light profile
        // withholds 35 of ~48 tools), it makes the retry worth making — the
        // model gets something it did not have — and it self-limits, since after
        // one reveal there is nothing left to reveal and the branch cannot fire
        // again. A full-catalog session that answers "I'll help with that" is
        // untouched.
        if assistant.tool_calls.is_empty()
            && !retry.promise_retried
            && assistant
                .content
                .as_deref()
                .is_some_and(super::promise_check::promises_action)
        {
            let revealed = self.catalog.reveal_all_deferred();
            if revealed > 0 {
                retry.promise_retried = true;
                retry.repair_promised_work = true;
                *definitions = self.catalog.definitions();
                self.note_cache_reset("tiering");
                tracing::warn!(
                    model = self.provider.model(),
                    revealed,
                    "model promised work without calling a tool — revealing deferred tools, retrying once"
                );
                return Ok(OutputCheck::Retry);
            }
        }
        retry.repair_reasoning_only = false;
        retry.repair_pseudo_tool = false;
        retry.repair_promised_work = false;
        Ok(OutputCheck::Proceed)
    }
}
