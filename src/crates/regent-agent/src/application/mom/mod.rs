//! Mixture of Models (MoM): several **proposer** models answer one brief in
//! parallel (advisory — no tools, no agent loop), then an **aggregator** model
//! synthesizes their answers into one. This is the Mixture-of-Agents technique
//! (Together's paper; mirrors Hermes `moa_loop.py`) with *model-level*
//! proposers — the diversity that matters is model diversity, so a proposer is
//! just a `ChatProvider` (resolved from a `ModelRef` via item A's registry by
//! the caller — this runner stays free of provider-config types). Named MoM,
//! not MoA, to reflect that the units are models, not full agents.
//!
//! Proposers run through the same bounded, order-preserving fan-out
//! `delegate_task` uses (`buffered`). A proposer that errors is **dropped**, not
//! fatal — the aggregator still synthesizes from the survivors (partial context
//! beats none). With zero surviving proposals the aggregator answers the brief
//! alone (a disabled MoM = just the aggregator).

use futures::StreamExt;
use regent_kernel::{ChatMessage, RegentError};
use regent_providers::{ChatProvider, ChatRequest};
use std::sync::Arc;

const DEFAULT_PROPOSER_PROMPT: &str =
    "You are a proposer in a Mixture-of-Models process. Answer the user's request \
     directly, completely, and independently. Another model will synthesize your \
     answer together with other proposers' answers.";

const DEFAULT_AGGREGATOR_PROMPT: &str =
    "You are the aggregator in a Mixture-of-Models process. You are given several \
     proposers' independent answers to the user's request. Synthesize them into one \
     best answer: keep what is correct, resolve disagreements, drop what is wrong, \
     and fill gaps. Answer the user directly — do not mention the proposers.";

/// Default upper bound on proposers (cost ceiling — every proposer is a model
/// call). Matches `DelegationConfig::max_concurrent`.
const DEFAULT_MAX_PROPOSERS: usize = 3;

pub struct MomRunner {
    proposers: Vec<Arc<dyn ChatProvider>>,
    aggregator: Arc<dyn ChatProvider>,
    proposer_prompt: String,
    aggregator_prompt: String,
    max_proposers: usize,
    max_tokens: u32,
}

impl MomRunner {
    /// `proposers` are pre-resolved providers (the caller resolves `ModelRef`s
    /// through the provider registry). `aggregator` synthesizes their answers.
    #[must_use]
    pub fn new(proposers: Vec<Arc<dyn ChatProvider>>, aggregator: Arc<dyn ChatProvider>) -> Self {
        Self {
            proposers,
            aggregator,
            proposer_prompt: DEFAULT_PROPOSER_PROMPT.to_owned(),
            aggregator_prompt: DEFAULT_AGGREGATOR_PROMPT.to_owned(),
            max_proposers: DEFAULT_MAX_PROPOSERS,
            max_tokens: 4096,
        }
    }

    #[must_use]
    pub fn with_max_proposers(mut self, n: usize) -> Self {
        self.max_proposers = n.max(1);
        self
    }

    #[must_use]
    pub fn with_prompts(
        mut self,
        proposer: impl Into<String>,
        aggregator: impl Into<String>,
    ) -> Self {
        self.proposer_prompt = proposer.into();
        self.aggregator_prompt = aggregator.into();
        self
    }

    /// Run the MoM: fan out proposers (capped, advisory), then aggregate.
    pub async fn run(&self, brief: &str) -> Result<String, RegentError> {
        let cap = self.max_proposers.min(self.proposers.len());
        let selected = &self.proposers[..cap];
        tracing::info!(proposers = selected.len(), "mom fan-out");

        // Bounded, order-preserving fan-out — same primitive as delegate_task.
        let prompt = self.proposer_prompt.as_str();
        let max_tokens = self.max_tokens;
        let proposals: Vec<Option<String>> =
            futures::stream::iter(selected.iter().map(|p| advise(p, prompt, brief, max_tokens)))
                .buffered(cap.max(1))
                .collect()
                .await;

        // Drop failed/empty proposers; the aggregator works from the survivors.
        let proposals: Vec<String> = proposals.into_iter().flatten().collect();
        tracing::info!(survived = proposals.len(), "mom proposals collected");

        let agg_brief = aggregator_brief(brief, &proposals);
        match advise(
            &self.aggregator,
            &self.aggregator_prompt,
            &agg_brief,
            self.max_tokens,
        )
        .await
        {
            Some(text) => Ok(text),
            None => Err(RegentError::Provider(
                "mom aggregator produced no output".into(),
            )),
        }
    }
}

/// One advisory model call: a single completion, no tools. Returns the assistant
/// text, or `None` on failure/empty (logged) so a proposer never aborts the run.
/// (As a free function, not a method, so the fan-out closures don't borrow self.)
async fn advise(
    provider: &Arc<dyn ChatProvider>,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Option<String> {
    let mut request = ChatRequest::new(system, vec![ChatMessage::user(user)]);
    request.max_tokens = Some(max_tokens);
    match provider.complete(&request).await {
        Ok(response) => response.message.content.filter(|t| !t.trim().is_empty()),
        Err(error) => {
            tracing::warn!(model = provider.model(), %error, "mom proposer failed; skipping");
            None
        }
    }
}

/// Pure: assemble the aggregator's brief from the original brief + the proposers'
/// answers (the "reference responses"). Stable `Proposal {n}` labels.
fn aggregator_brief(brief: &str, proposals: &[String]) -> String {
    if proposals.is_empty() {
        return brief.to_owned();
    }
    let joined: String = proposals
        .iter()
        .enumerate()
        .map(|(i, text)| format!("Proposal {}:\n{}", i + 1, text))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Original request:\n{brief}\n\nProposer answers:\n{joined}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use regent_providers::{ChatResponse, ProviderError};
    use std::sync::Mutex;

    /// Returns a fixed reply (or errors), and records the last request's user
    /// text so a test can assert what the aggregator actually saw.
    struct Mock {
        reply: Option<&'static str>, // None ⇒ error (a failing proposer)
        last_user: Mutex<Option<String>>,
    }
    impl Mock {
        fn ok(reply: &'static str) -> Arc<Self> {
            Arc::new(Self {
                reply: Some(reply),
                last_user: Mutex::new(None),
            })
        }
        fn failing() -> Arc<Self> {
            Arc::new(Self {
                reply: None,
                last_user: Mutex::new(None),
            })
        }
    }
    #[async_trait]
    impl ChatProvider for Mock {
        async fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            let user = request
                .messages
                .last()
                .and_then(|m| m.content.clone())
                .unwrap_or_default();
            *self.last_user.lock().unwrap() = Some(user);
            match self.reply {
                Some(text) => Ok(ChatResponse {
                    message: ChatMessage::assistant(Some(text.to_owned()), vec![]),
                    usage: or_core::TokenUsage::default(),
                    finish_reason: Some("stop".into()),
                }),
                None => Err(ProviderError::Parse("boom".into())),
            }
        }
        fn model(&self) -> &str {
            "mock"
        }
    }

    #[test]
    fn aggregator_brief_includes_every_proposal_and_the_brief() {
        let b = aggregator_brief("do X", &["a".into(), "b".into(), "c".into()]);
        assert!(b.contains("do X"));
        assert!(b.contains("Proposal 1:\na"));
        assert!(b.contains("Proposal 2:\nb"));
        assert!(b.contains("Proposal 3:\nc"));
    }

    #[test]
    fn aggregator_brief_with_no_proposals_is_just_the_brief() {
        assert_eq!(aggregator_brief("do X", &[]), "do X");
    }

    #[tokio::test]
    async fn aggregator_sees_all_proposals_and_returns_its_synthesis() {
        let agg = Mock::ok("synthesized");
        let runner = MomRunner::new(
            vec![Mock::ok("alpha"), Mock::ok("beta"), Mock::ok("gamma")],
            Arc::clone(&agg) as Arc<dyn ChatProvider>,
        );
        let out = runner.run("the brief").await.unwrap();
        assert_eq!(out, "synthesized");
        let seen = agg.last_user.lock().unwrap().clone().unwrap();
        assert!(seen.contains("alpha") && seen.contains("beta") && seen.contains("gamma"));
        assert!(seen.contains("the brief"));
    }

    #[tokio::test]
    async fn a_failing_proposer_is_skipped_not_fatal() {
        let agg = Mock::ok("synthesized");
        let runner = MomRunner::new(
            vec![Mock::ok("alpha"), Mock::failing(), Mock::ok("gamma")],
            Arc::clone(&agg) as Arc<dyn ChatProvider>,
        );
        let out = runner.run("brief").await.unwrap();
        assert_eq!(out, "synthesized");
        let seen = agg.last_user.lock().unwrap().clone().unwrap();
        assert!(seen.contains("alpha") && seen.contains("gamma"));
        assert!(!seen.contains("Proposal 3"), "only 2 survived");
    }

    #[tokio::test]
    async fn max_proposers_caps_the_fan_out() {
        let agg = Mock::ok("synthesized");
        let runner = MomRunner::new(
            vec![Mock::ok("a"), Mock::ok("b"), Mock::ok("c"), Mock::ok("d")],
            Arc::clone(&agg) as Arc<dyn ChatProvider>,
        )
        .with_max_proposers(2);
        runner.run("brief").await.unwrap();
        let seen = agg.last_user.lock().unwrap().clone().unwrap();
        assert!(
            seen.contains("Proposal 2") && !seen.contains("Proposal 3"),
            "capped at 2"
        );
    }
}
