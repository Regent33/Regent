use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// SKILL.md frontmatter (agentskills.io-compatible subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillMeta {
    pub name: String,
    /// Hardline standard: ≤ 60 chars, one sentence, ends with a period.
    pub description: String,
    pub version: String,
    /// `agent` | `user` | `bundled` — the curator only ever touches `agent`.
    pub created_by: String,
    /// Pinned skills are exempt from every automatic lifecycle transition.
    pub pinned: bool,
    pub tags: Vec<String>,
}

impl SkillMeta {
    #[must_use]
    pub fn new(name: impl Into<String>, description: impl Into<String>, created_by: &str) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            version: "0.1.0".to_owned(),
            created_by: created_by.to_owned(),
            pinned: false,
            tags: Vec::new(),
        }
    }
}

/// Level-0 listing entry (what the prompt index shows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// A fully loaded skill (level 1) plus its reference files (level 2 paths).
#[derive(Debug, Clone)]
pub struct SkillRecord {
    pub meta: SkillMeta,
    pub body: String,
    /// Relative paths of extra files under the skill directory.
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SkillState {
    #[default]
    Active,
    Stale,
    Archived,
}

/// Per-skill telemetry — the substrate the curator decides on.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageRecord {
    pub use_count: u64,
    pub view_count: u64,
    pub patch_count: u64,
    /// Unix epoch seconds of the last view/use/patch/create.
    pub last_activity_at: f64,
    #[serde(default)]
    pub state: SkillState,
}

/// The `.usage.json` sidecar content (BTreeMap for stable serialization).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageLog {
    pub skills: BTreeMap<String, UsageRecord>,
}

impl UsageLog {
    pub fn touch(&mut self, name: &str, now: f64, bump: impl FnOnce(&mut UsageRecord)) {
        let record = self.skills.entry(name.to_owned()).or_default();
        bump(record);
        record.last_activity_at = now;
    }
}
