//! The model-facing skills index (MRU cap + overflow pointer). Split from
//! `library.rs` (file-size rule).

use super::library::SKILLS_INDEX_MAX;
use super::library::SkillLibrary;
use crate::domain::entities::{SkillSummary, UsageLog};
use crate::domain::errors::SkillError;
use std::collections::HashSet;

impl SkillLibrary {
    /// MRU cap (SPL §3.4): past the threshold, only the most-recently-used
    /// skills' lines render — the index is paid for on every request and would
    /// otherwise grow without bound as skills accumulate. The rest stay one
    /// `skills_list` call away; a closing line says so. Never-used skills rank
    /// as 0.0 (they earn residency by being used).
    fn apply_mru_cap(summaries: &mut Vec<SkillSummary>, usage: &UsageLog) {
        if summaries.len() <= SKILLS_INDEX_MAX {
            return;
        }
        summaries.sort_by(|a, b| {
            let at = |s: &SkillSummary| {
                usage
                    .skills
                    .get(&s.name)
                    .map_or(0.0, |r| r.last_activity_at)
            };
            at(b)
                .partial_cmp(&at(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.name.cmp(&b.name))
        });
        summaries.truncate(SKILLS_INDEX_MAX);
        // Name order among the kept set: a same-set rebuild renders the same
        // bytes regardless of intra-set recency shuffles.
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// The names the index would render, **from a caller-supplied snapshot**.
    ///
    /// Pure: no I/O, so the curator can decide visibility from the exact same
    /// `list()`/`load_usage()` pair it judges idleness with. Re-reading here
    /// instead would let one plan mix two views of the library [co-audit].
    #[must_use]
    pub fn visible_from(summaries: &[SkillSummary], usage: &UsageLog) -> HashSet<String> {
        let mut ranked = summaries.to_vec();
        Self::apply_mru_cap(&mut ranked, usage);
        ranked.into_iter().map(|s| s.name).collect()
    }

    /// The names the index would render right now. Convenience wrapper for
    /// callers that do not already hold a snapshot.
    pub fn index_visible_names(&self) -> Result<HashSet<String>, SkillError> {
        let summaries = self.list()?;
        let usage = self.repository.load_usage()?;
        Ok(Self::visible_from(&summaries, &usage))
    }

    /// Stable-tier prompt block: compact index, byte-stable ordering.
    pub fn render_index(&self) -> Result<String, SkillError> {
        let mut summaries = self.list()?;
        if summaries.is_empty() {
            return Ok(String::new());
        }
        let total = summaries.len();
        if total > SKILLS_INDEX_MAX {
            let usage = self.repository.load_usage()?;
            Self::apply_mru_cap(&mut summaries, &usage);
        }
        let mut out = String::from(
            "## Skills\nBefore acting, scan this index. If a skill clearly matches the task, \
             load it with skill_view(name) and follow it.\n<available_skills>\n",
        );
        for summary in &summaries {
            // The index is paid for on every request — cap each hook; the full
            // description still arrives with the body via skill_view.
            let hook: String = if summary.description.chars().count() > 140 {
                let mut s: String = summary.description.chars().take(139).collect();
                s.push('…');
                s
            } else {
                summary.description.clone()
            };
            out.push_str(&format!("- {}: {hook}\n", summary.name));
        }
        if total > SKILLS_INDEX_MAX {
            out.push_str(&format!(
                "- …and {} more — skills_list shows all.\n",
                total - SKILLS_INDEX_MAX
            ));
        }
        out.push_str("</available_skills>");
        Ok(out)
    }

    pub(super) fn record_activity(
        &self,
        name: &str,
        bump: impl FnOnce(&mut crate::domain::entities::UsageRecord),
    ) -> Result<(), SkillError> {
        let mut usage = self.repository.load_usage()?;
        usage.touch(name, (self.now)(), bump);
        self.repository.save_usage(&usage)
    }
}
