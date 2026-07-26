//! Filesystem `SkillRepository`: one directory per skill with SKILL.md,
//! optional reference files, `.archive/` for retired skills, and the
//! `.usage.json` telemetry sidecar.

use crate::domain::contracts::SkillRepository;
use crate::domain::entities::{SkillMeta, SkillRecord, UsageLog};
use crate::domain::errors::SkillError;
use crate::infra::frontmatter;
use std::path::{Path, PathBuf};

pub struct FsSkillRepository {
    root: PathBuf,
}

impl FsSkillRepository {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, SkillError> {
        let root = root.into();
        std::fs::create_dir_all(root.join(".archive"))?;
        Ok(Self { root })
    }

    fn skill_dir(&self, name: &str) -> Result<PathBuf, SkillError> {
        // Directory traversal guard — names were validated upstream, but the
        // repository defends its own boundary too.
        if name.contains(['/', '\\', '.']) || name.is_empty() {
            return Err(SkillError::PathEscape(name.to_owned()));
        }
        Ok(self.root.join(name))
    }

    fn usage_path(&self) -> PathBuf {
        self.root.join(".usage.json")
    }
}

impl SkillRepository for FsSkillRepository {
    fn list_names(&self) -> Result<Vec<String>, SkillError> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_dir()
                && !name.starts_with('.')
                && entry.path().join("SKILL.md").exists()
            {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    fn exists(&self, name: &str) -> bool {
        self.skill_dir(name)
            .map(|dir| dir.join("SKILL.md").exists())
            .unwrap_or(false)
    }

    fn load(&self, name: &str) -> Result<SkillRecord, SkillError> {
        let active = self.skill_dir(name)?;
        if active.join("SKILL.md").exists() {
            return load_record(&active, name);
        }
        // Fall back to the archive so a retired (opted-out) skill can still be
        // VIEWED by name — the Skills UI lists archived rows and clicking one
        // must show its body, not "skill not found". Discovery/list stay
        // active-only (name is already separator-validated by skill_dir).
        let archived = self.root.join(".archive").join(name);
        if archived.join("SKILL.md").exists() {
            return load_record(&archived, name);
        }
        load_record(&active, name) // preserves the NotFound(name) error
    }

    fn list_archived(&self) -> Result<Vec<SkillRecord>, SkillError> {
        let archive = self.root.join(".archive");
        let mut records = Vec::new();
        let entries = match std::fs::read_dir(&archive) {
            Ok(entries) => entries,
            // No `.archive/` yet → nothing retired (create() makes it lazily).
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(records),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let dir = entry.path();
            if entry.file_type()?.is_dir() && dir.join("SKILL.md").exists() {
                match load_record(&dir, &name) {
                    Ok(record) => records.push(record),
                    Err(error) => {
                        tracing::warn!(skill = name, %error, "skipping unreadable archived skill");
                    }
                }
            }
        }
        records.sort_by(|a, b| a.meta.name.cmp(&b.meta.name));
        Ok(records)
    }

    fn save(&self, meta: &SkillMeta, body: &str) -> Result<(), SkillError> {
        let dir = self.skill_dir(&meta.name)?;
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("SKILL.md"), frontmatter::render(meta, body))?;
        Ok(())
    }

    fn read_file(&self, name: &str, relative: &str) -> Result<String, SkillError> {
        let dir = self.skill_dir(name)?;
        let candidate = dir.join(relative);
        // Containment check on the resolved path (level-2 disclosure must
        // never escape the skill directory).
        let resolved = candidate
            .canonicalize()
            .map_err(|_| SkillError::NotFound(format!("{name}/{relative}")))?;
        let base = dir
            .canonicalize()
            .map_err(|_| SkillError::NotFound(name.to_owned()))?;
        if !resolved.starts_with(&base) {
            return Err(SkillError::PathEscape(relative.to_owned()));
        }
        Ok(std::fs::read_to_string(resolved)?)
    }

    fn archive(&self, name: &str) -> Result<(), SkillError> {
        let source = self.skill_dir(name)?;
        if !source.exists() {
            return Err(SkillError::NotFound(name.to_owned()));
        }
        let mut target = self.root.join(".archive").join(name);
        let mut suffix = 1;
        while target.exists() {
            target = self.root.join(".archive").join(format!("{name}-{suffix}"));
            suffix += 1;
        }
        std::fs::rename(&source, &target)?;
        tracing::info!(skill = name, target = %target.display(), "skill archived");
        Ok(())
    }

    fn unarchive(&self, name: &str) -> Result<(), SkillError> {
        let target = self.skill_dir(name)?;
        // An active skill of the same name would be silently clobbered by the
        // rename — refuse instead (mirror of create's exists guard).
        if target.exists() {
            return Err(SkillError::AlreadyExists(name.to_owned()));
        }
        let source = self.root.join(".archive").join(name);
        if !source.exists() {
            return Err(SkillError::NotFound(name.to_owned()));
        }
        std::fs::rename(&source, &target)?;
        tracing::info!(skill = name, "skill unarchived");
        Ok(())
    }

    fn load_usage(&self) -> Result<UsageLog, SkillError> {
        match std::fs::read_to_string(self.usage_path()) {
            // A corrupt telemetry sidecar must never make skills unviewable.
            Ok(raw) => Ok(parse_usage(&raw)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(UsageLog::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn save_usage(&self, log: &UsageLog) -> Result<(), SkillError> {
        let raw =
            serde_json::to_string_pretty(log).map_err(|e| SkillError::Storage(e.to_string()))?;
        // Atomic temp→rename: a plain in-place write tears under concurrent
        // deacons (every CLI command spawns one) — a shorter JSON written over
        // a longer one leaves trailing garbage, and load_usage's reset then
        // wipes the whole usage history ("corrupt .usage.json" in the logs).
        let tmp = self
            .root
            .join(format!(".usage.json.tmp.{}", std::process::id()));
        std::fs::write(&tmp, raw)?;
        Ok(std::fs::rename(&tmp, self.usage_path())?)
    }
}

/// Reads the usage ledger, salvaging what a torn write left readable.
///
/// The observed corruption is always `trailing characters at line N` — a
/// shorter ledger written over a longer one, so the leading object is INTACT and
/// holds essentially the whole history. Resetting to `default()` on that threw
/// it all away, and the 30-day window it feeds is what adaptive tool tiering
/// decides residency from: a wipe re-defers every tool, which the model then has
/// to claw back through reveal-on-stuck (8 wipes and 129 reveals in one day on
/// the reporting machine). `StreamDeserializer` reads exactly one value and
/// ignores whatever follows, which is precisely the salvage we want.
///
/// A ledger that is unreadable from the very first byte is genuinely lost, and
/// only that case starts over.
fn parse_usage(raw: &str) -> UsageLog {
    if let Ok(log) = serde_json::from_str::<UsageLog>(raw) {
        return log;
    }
    match serde_json::Deserializer::from_str(raw)
        .into_iter::<UsageLog>()
        .next()
    {
        Some(Ok(log)) => {
            tracing::warn!("torn .usage.json — salvaged the readable ledger, dropped the tail");
            log
        }
        _ => {
            tracing::warn!("unreadable .usage.json — starting a fresh usage ledger");
            UsageLog::default()
        }
    }
}

/// Parses a skill directory (active or archived) into a record; `name` is the
/// directory identity (frontmatter `name` is overridden by it).
fn load_record(dir: &Path, name: &str) -> Result<SkillRecord, SkillError> {
    let raw = std::fs::read_to_string(dir.join("SKILL.md"))
        .map_err(|_| SkillError::NotFound(name.to_owned()))?;
    let (mut meta, body) = frontmatter::parse(&raw)?;
    meta.name = name.to_owned();
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.retain(|f| f != "SKILL.md");
    files.sort();
    Ok(SkillRecord { meta, body, files })
}

fn collect_files(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), SkillError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files(base, &path, out)?;
        } else if let Ok(relative) = path.strip_prefix(base) {
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod usage_tests {
    use super::parse_usage;

    const GOOD: &str = r#"{"skills":{"alpha":{"use_count":3,"view_count":1,"patch_count":0,"last_activity_at":1.0,"state":"active"}}}"#;

    #[test]
    fn a_clean_ledger_round_trips() {
        let log = parse_usage(GOOD);
        assert_eq!(log.skills.len(), 1);
        assert_eq!(log.skills["alpha"].use_count, 3);
    }

    #[test]
    fn a_torn_write_keeps_the_readable_ledger_instead_of_wiping_it() {
        // The exact observed shape: a shorter ledger written over a longer one,
        // leaving a valid object followed by the old tail ("trailing
        // characters at line N"). Every count must survive — this window is
        // what tool tiering decides residency from.
        let torn = format!("{GOOD}\"state\": \"active\"}}}}}}");
        let log = parse_usage(&torn);
        assert_eq!(log.skills.len(), 1, "salvaged, not reset");
        assert_eq!(log.skills["alpha"].use_count, 3);
    }

    #[test]
    fn garbage_from_the_first_byte_starts_over() {
        assert!(parse_usage("not json at all").skills.is_empty());
        assert!(parse_usage("").skills.is_empty());
    }
}
