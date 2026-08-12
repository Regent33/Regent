//! The bounded prompt stores — MEMORY/USER semantics: hard char
//! budgets, no auto-compaction (over-budget writes error with the current
//! entries so the agent consolidates in the same turn), substring-matched
//! replace/remove, exact duplicates are a friendly no-op.

use crate::application::orchestrators::GraphMemory;
use crate::domain::entities::{AddOutcome, MemoryTarget, Provenance};
use crate::domain::errors::GraphError;
use crate::domain::policy;

/// The per-turn cost of the always-injected block, for one target (W3 step 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockMetrics {
    pub target: MemoryTarget,
    pub entries: usize,
    pub chars: usize,
    pub limit: usize,
}

impl BlockMetrics {
    /// How full the store is. The live store sat at 75% with six entries, which
    /// is what makes narrowing urgent rather than theoretical.
    #[must_use]
    pub fn percent_full(&self) -> usize {
        (self.chars * 100).checked_div(self.limit).unwrap_or(0)
    }
}

impl GraphMemory {
    pub fn add_entry(&self, target: MemoryTarget, content: &str) -> Result<AddOutcome, GraphError> {
        policy::validate_content(content)?;
        let entries = self.entry_nodes(target)?;
        if entries.iter().any(|(_, text)| text == content) {
            return Ok(AddOutcome::Duplicate);
        }
        let used: usize = entries.iter().map(|(_, text)| text.len()).sum();
        let attempted = content.len();
        let limit = self.budget(target);
        if used + attempted > limit {
            return Err(GraphError::BudgetExceeded {
                used,
                limit,
                attempted,
                entries: entries.into_iter().map(|(_, text)| text).collect(),
            });
        }
        self.add_node(
            target.kind(),
            "",
            content,
            Provenance::AgentInferred,
            None,
            None,
        )?;
        Ok(AddOutcome::Added)
    }

    pub fn replace_entry(
        &self,
        target: MemoryTarget,
        old_text: &str,
        content: &str,
    ) -> Result<(), GraphError> {
        policy::validate_content(content)?;
        let entries = self.entry_nodes(target)?;
        let (node_id, old_content) = match_one(&entries, old_text)?;
        // `replace` is bound by the budget too: a longer entry can overflow.
        let used: usize = entries.iter().map(|(_, text)| text.len()).sum();
        let new_used = used - old_content.len() + content.len();
        let limit = self.budget(target);
        // A store that is ALREADY over budget must still accept a shrinking
        // replace. Rows written under the old char budget can exceed the byte
        // one without a single new write, and a plain `new_used > limit` then
        // refused every edit — including the consolidation the error itself
        // tells the agent to perform ("replace overlapping entries with shorter
        // ones"), leaving `remove` as the only move that could ever succeed.
        // Growing writes are still refused; shrinking ones always move toward
        // the limit, which is the direction the guard exists to enforce.
        if new_used > limit && content.len() > old_content.len() {
            return Err(GraphError::BudgetExceeded {
                used,
                limit,
                attempted: content.len(),
                entries: entries.into_iter().map(|(_, text)| text).collect(),
            });
        }
        let hash = policy::content_hash(target.kind(), "", content);
        self.store.update_node_content(&node_id, content, &hash)?;
        Ok(())
    }

    pub fn remove_entry(&self, target: MemoryTarget, old_text: &str) -> Result<(), GraphError> {
        let entries = self.entry_nodes(target)?;
        let (node_id, _) = match_one(&entries, old_text)?;
        self.store.delete_node(&node_id)?;
        Ok(())
    }

    pub fn entries(&self, target: MemoryTarget) -> Result<Vec<String>, GraphError> {
        Ok(self
            .entry_nodes(target)?
            .into_iter()
            .map(|(_, text)| text)
            .collect())
    }

    pub fn usage(&self, target: MemoryTarget) -> Result<(usize, usize), GraphError> {
        let used = self
            .entry_nodes(target)?
            .iter()
            .map(|(_, text)| text.len())
            .sum();
        Ok((used, self.budget(target)))
    }

    /// What the always-injected block costs, per target (W3 step 1).
    ///
    /// The baseline every later step is measured against. Today this block is
    /// the whole corpus, injected on every turn whatever the question — so its
    /// cost is paid per turn while its relevance to any given turn is unknown.
    /// Freezing the number is the precondition for narrowing it.
    pub fn block_metrics(&self) -> Result<Vec<BlockMetrics>, GraphError> {
        [MemoryTarget::Memory, MemoryTarget::User]
            .into_iter()
            .map(|target| {
                let (used, limit) = self.usage(target)?;
                Ok(BlockMetrics {
                    target,
                    entries: self.entry_nodes(target)?.len(),
                    chars: used,
                    limit,
                })
            })
            .collect()
    }

    /// The frozen prompt block — captured once at session start (the
    /// pattern: live writes hit the store immediately, the prompt sees them
    /// next session).
    pub fn render_prompt_block(&self) -> Result<String, GraphError> {
        let memory = self.render_store(MemoryTarget::Memory, "MEMORY (your personal notes)")?;
        let user = self.render_store(MemoryTarget::User, "USER PROFILE")?;
        Ok(format!("{memory}\n\n{user}"))
    }

    fn render_store(&self, target: MemoryTarget, title: &str) -> Result<String, GraphError> {
        let entries = self.entries(target)?;
        let (used, limit) = self.usage(target)?;
        let percent = (used * 100).checked_div(limit).unwrap_or(0);
        let bar = "═".repeat(46);
        let body = if entries.is_empty() {
            "(empty)".to_owned()
        } else {
            entries.join("\n§\n")
        };
        Ok(format!(
            "{bar}\n{title} [{percent}% — {used}/{limit} bytes]\n{bar}\n{body}"
        ))
    }

    /// Entry rows for a target, insertion-ordered, as (node_id, content).
    fn entry_nodes(&self, target: MemoryTarget) -> Result<Vec<(String, String)>, GraphError> {
        Ok(self
            .store
            .nodes_by_kind(target.kind())?
            .into_iter()
            .map(|node| (node.id, node.content))
            .collect())
    }
}

/// Substring matching with strict semantics: exactly one entry must match.
fn match_one(entries: &[(String, String)], old_text: &str) -> Result<(String, String), GraphError> {
    let matches: Vec<&(String, String)> = entries
        .iter()
        .filter(|(_, text)| text.contains(old_text))
        .collect();
    match matches.as_slice() {
        [] => Err(GraphError::NoMatch(old_text.to_owned())),
        [single] => Ok((single.0.clone(), single.1.clone())),
        _ => Err(GraphError::AmbiguousMatch(old_text.to_owned())),
    }
}
