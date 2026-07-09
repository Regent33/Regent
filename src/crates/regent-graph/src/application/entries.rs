//! The bounded prompt stores — MEMORY/USER semantics: hard char
//! budgets, no auto-compaction (over-budget writes error with the current
//! entries so the agent consolidates in the same turn), substring-matched
//! replace/remove, exact duplicates are a friendly no-op.

use crate::application::orchestrators::GraphMemory;
use crate::domain::entities::{AddOutcome, MemoryTarget, Provenance};
use crate::domain::errors::GraphError;
use crate::domain::policy;

impl GraphMemory {
    pub fn add_entry(&self, target: MemoryTarget, content: &str) -> Result<AddOutcome, GraphError> {
        policy::validate_content(content)?;
        let entries = self.entry_nodes(target)?;
        if entries.iter().any(|(_, text)| text == content) {
            return Ok(AddOutcome::Duplicate);
        }
        let used: usize = entries.iter().map(|(_, text)| text.chars().count()).sum();
        let attempted = content.chars().count();
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
        let used: usize = entries.iter().map(|(_, text)| text.chars().count()).sum();
        let new_used = used - old_content.chars().count() + content.chars().count();
        let limit = self.budget(target);
        if new_used > limit {
            return Err(GraphError::BudgetExceeded {
                used,
                limit,
                attempted: content.chars().count(),
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
            .map(|(_, text)| text.chars().count())
            .sum();
        Ok((used, self.budget(target)))
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
            "{bar}\n{title} [{percent}% — {used}/{limit} chars]\n{bar}\n{body}"
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
