use crate::domain::entities::{SkillMeta, SkillRecord, UsageLog};
use crate::domain::errors::SkillError;

/// Persistence contract for skills (interfaces live in domain; the
/// filesystem implementation lives in infra). Synchronous by design —
/// callers bridge off the async runtime like they do for the store.
pub trait SkillRepository: Send + Sync {
    /// Names of non-archived skills.
    fn list_names(&self) -> Result<Vec<String>, SkillError>;

    fn exists(&self, name: &str) -> bool;

    fn load(&self, name: &str) -> Result<SkillRecord, SkillError>;

    /// Creates or overwrites SKILL.md for `name`.
    fn save(&self, meta: &SkillMeta, body: &str) -> Result<(), SkillError>;

    /// Reads a reference file inside the skill directory (level-2
    /// disclosure). Implementations MUST contain the path.
    fn read_file(&self, name: &str, relative: &str) -> Result<String, SkillError>;

    /// Moves the skill out of the active set (never deletes).
    fn archive(&self, name: &str) -> Result<(), SkillError>;

    /// Restores a previously archived skill to the active set (inverse of
    /// [`Self::archive`]).
    fn unarchive(&self, name: &str) -> Result<(), SkillError>;

    /// Full records for archived skills (the opt-in surface). Empty when
    /// nothing has been retired.
    fn list_archived(&self) -> Result<Vec<SkillRecord>, SkillError>;

    fn load_usage(&self) -> Result<UsageLog, SkillError>;

    fn save_usage(&self, log: &UsageLog) -> Result<(), SkillError>;
}
