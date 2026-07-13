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

use regent_kernel::{ChatMessage, RegentError};
use regent_providers::{ChatProvider, ChatRequest};
use std::sync::Arc;

const DEFAULT_PROPOSER_PROMPT: &str = "You are a proposer in a Mixture-of-Models process. Answer the user's request \
     directly, completely, and independently. Another model will synthesize your \
     answer together with other proposers' answers.";

const DEFAULT_AGGREGATOR_PROMPT: &str = "You are the aggregator in a Mixture-of-Models process. You are given several \
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
        let max_tokens = self.max_tokens;
        tracing::info!(proposers = cap, "mom fan-out");

        // Build owned proposer futures eagerly (a for-loop, not a lazy `.map`
        // closure — the closure would make the awaited stream borrow `&self`,
        // and the resulting future non-Send under nested async_trait). Each
        // future owns its inputs, so they are Send + 'static. Concurrency is
        // bounded by `cap` (= max_proposers, the cost ceiling).
        let mut calls = Vec::with_capacity(cap);
        for provider in self.proposers[..cap].iter() {
            let provider = Arc::clone(provider);
            let system = self.proposer_prompt.clone();
            let user = brief.to_owned();
            calls.push(async move { advise(&provider, &system, &user, max_tokens).await });
        }
        let proposals: Vec<Option<String>> = futures::future::join_all(calls).await;

        // Drop failed/empty proposers; the aggregator works from the survivors.
        let proposals: Vec<String> = proposals.into_iter().flatten().collect();
        tracing::info!(survived = proposals.len(), "mom proposals collected");

        let agg_brief = aggregator_brief(brief, &proposals);
        let aggregator = Arc::clone(&self.aggregator);
        let agg_prompt = self.aggregator_prompt.clone();
        match advise(&aggregator, &agg_prompt, &agg_brief, max_tokens).await {
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
#[path = "mod_tests.rs"]
mod tests;
