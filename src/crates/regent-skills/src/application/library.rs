//! Library use cases over the [`SkillRepository`] contract: progressive
//! disclosure (list → view → file), agent-driven create/patch, archive,
//! and the stable-tier prompt index. All telemetry flows through here.

use crate::domain::contracts::SkillRepository;
use crate::domain::entities::{SkillMeta, SkillRecord, SkillSummary};
use crate::domain::errors::SkillError;
use std::sync::Arc;

pub struct SkillLibrary {
    repository: Arc<dyn SkillRepository>,
    now: fn() -> f64,
}

fn epoch_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

impl SkillLibrary {
    #[must_use]
    pub fn new(repository: Arc<dyn SkillRepository>) -> Self {
        Self { repository, now: epoch_now }
    }

    /// Test seam: inject a deterministic clock.
    #[must_use]
    pub fn with_clock(mut self, now: fn() -> f64) -> Self {
        self.now = now;
        self
    }

    #[must_use]
    pub fn repository(&self) -> &Arc<dyn SkillRepository> {
        &self.repository
    }

    /// Level 0: name + description index.
    pub fn list(&self) -> Result<Vec<SkillSummary>, SkillError> {
        let mut summaries = Vec::new();
        for name in self.repository.list_names()? {
            match self.repository.load(&name) {
                Ok(record) => summaries.push(SkillSummary {
                    name: record.meta.name,
                    description: record.meta.description,
                    tags: record.meta.tags,
                }),
                Err(error) => tracing::warn!(skill = name, %error, "skipping unreadable skill"),
            }
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }

    /// Level 1: full skill content. Counts as a view.
    pub fn view(&self, name: &str) -> Result<SkillRecord, SkillError> {
        let record = self.repository.load(name)?;
        self.record_activity(name, |r| r.view_count += 1)?;
        Ok(record)
    }

    /// Level 2: one reference file inside the skill.
    pub fn view_file(&self, name: &str, relative: &str) -> Result<String, SkillError> {
        let content = self.repository.read_file(name, relative)?;
        self.record_activity(name, |r| r.view_count += 1)?;
        Ok(content)
    }

    /// Creates a new skill (agent provenance by default for tool-driven
    /// creation). Enforces the hardline naming/description standards.
    pub fn create(
        &self,
        name: &str,
        description: &str,
        body: &str,
        created_by: &str,
    ) -> Result<(), SkillError> {
        validate_name(name)?;
        validate_description(description)?;
        if self.repository.exists(name) {
            return Err(SkillError::AlreadyExists(name.to_owned()));
        }
        let meta = SkillMeta::new(name, description, created_by);
        self.repository.save(&meta, body)?;
        self.record_activity(name, |_| {})?;
        Ok(())
    }

    /// Replaces exactly one occurrence of `old_text` in the body.
    pub fn patch(&self, name: &str, old_text: &str, new_text: &str) -> Result<(), SkillError> {
        let record = self.repository.load(name)?;
        if record.body.matches(old_text).count() != 1 {
            return Err(SkillError::PatchMismatch(old_text.to_owned()));
        }
        let body = record.body.replacen(old_text, new_text, 1);
        self.repository.save(&record.meta, &body)?;
        self.record_activity(name, |r| r.patch_count += 1)?;
        Ok(())
    }

    /// Archive (never delete). Pinned skills refuse.
    pub fn archive(&self, name: &str) -> Result<(), SkillError> {
        let record = self.repository.load(name)?;
        if record.meta.pinned {
            return Err(SkillError::Pinned(name.to_owned()));
        }
        self.repository.archive(name)?;
        let mut usage = self.repository.load_usage()?;
        usage.touch(name, (self.now)(), |r| {
            r.state = crate::domain::entities::SkillState::Archived;
        });
        self.repository.save_usage(&usage)
    }

    /// Marks a slash-command / workflow invocation.
    pub fn record_use(&self, name: &str) -> Result<(), SkillError> {
        self.record_activity(name, |r| r.use_count += 1)
    }

    /// Stable-tier prompt block: compact index, byte-stable ordering.
    pub fn render_index(&self) -> Result<String, SkillError> {
        let summaries = self.list()?;
        if summaries.is_empty() {
            return Ok(String::new());
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
        out.push_str("</available_skills>");
        Ok(out)
    }

    fn record_activity(
        &self,
        name: &str,
        bump: impl FnOnce(&mut crate::domain::entities::UsageRecord),
    ) -> Result<(), SkillError> {
        let mut usage = self.repository.load_usage()?;
        usage.touch(name, (self.now)(), bump);
        self.repository.save_usage(&usage)
    }
}

fn validate_name(name: &str) -> Result<(), SkillError> {
    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    let valid_start = name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if name.is_empty() || name.len() > 64 || !valid_chars || !valid_start {
        return Err(SkillError::Invalid {
            field: "name",
            reason: "must be 1-64 chars of [a-z0-9-_], starting alphanumeric".into(),
        });
    }
    Ok(())
}

fn validate_description(description: &str) -> Result<(), SkillError> {
    // Hardline standard: ≤60 chars, ends with a period — long descriptions
    // bloat the index and dilute attention when many skills load.
    let count = description.chars().count();
    if description.trim().is_empty() || count > 60 || !description.trim_end().ends_with('.') {
        return Err(SkillError::Invalid {
            field: "description",
            reason: format!("must be 1-60 chars ending with a period (got {count} chars)"),
        });
    }
    Ok(())
}
