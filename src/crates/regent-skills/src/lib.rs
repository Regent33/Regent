//! regent-skills — the procedural memory tier ("how to do X").
//!
//! Feature-based clean architecture, crate-internal:
//! - `domain/` — entities, the repository **contract**, typed errors. Zero I/O.
//! - `application/` — use cases: the library operations, the curator, the
//!   review prompt. Talks only to the domain contract.
//! - `infra/` — the filesystem repository implementation (SKILL.md +
//!   frontmatter + `.usage.json` sidecar), agentskills.io-compatible.

pub mod application;
pub mod domain;
pub mod infra;

pub use application::curator::{CuratorConfig, CuratorReport, curate};
pub use application::library::SkillLibrary;
pub use application::prompts::REVIEW_SYSTEM_PROMPT;
pub use domain::contracts::SkillRepository;
pub use domain::entities::{
    SkillMeta, SkillRecord, SkillState, SkillSummary, UsageLog, UsageRecord,
};
pub use domain::errors::SkillError;
pub use infra::fs_repository::FsSkillRepository;
