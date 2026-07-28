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
    /// Whether automatic lifecycle transitions may touch this skill — the
    /// curator's scope predicate, in one place so the rule cannot drift.
    /// Bundled, user-created and pinned skills are all out of scope.
    #[must_use]
    pub fn is_curatable(&self) -> bool {
        self.created_by == "agent" && !self.pinned
    }

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
    /// [`SkillMeta::is_curatable`], carried so the curator can plan from a
    /// single `list()` snapshot instead of re-loading every record to re-ask
    /// the same question. Free: `list` already has the full record in hand.
    pub curatable: bool,
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
    /// Epoch seconds since this skill has been *continuously* visible in the
    /// model-facing index, or `None` when it is not currently visible.
    ///
    /// The minimum-exposure guarantee. Current visibility alone says nothing
    /// about opportunity: a skill hidden by the index cap for 89 of its 90 idle
    /// days, then promoted because another skill was archived, is visible at
    /// the moment it is judged and would be retired on the spot. This records
    /// *how long* it has been reachable, so idleness can be read as a verdict
    /// rather than an artifact.
    ///
    /// Maintained by the curator, which already runs on a timer and already
    /// writes this file — deliberately NOT stamped at index-render time, which
    /// would put a whole-file write on the session-build path.
    ///
    /// `#[serde(default)]`: an existing `.usage.json` loads with `None` and
    /// starts accruing on the next pass.
    #[serde(default)]
    pub visible_since: Option<f64>,
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
