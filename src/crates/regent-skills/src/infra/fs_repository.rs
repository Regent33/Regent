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
        let dir = self.skill_dir(name)?;
        let raw = std::fs::read_to_string(dir.join("SKILL.md"))
            .map_err(|_| SkillError::NotFound(name.to_owned()))?;
        let (mut meta, body) = frontmatter::parse(&raw)?;
        meta.name = name.to_owned(); // directory is the identity
        let mut files = Vec::new();
        collect_files(&dir, &dir, &mut files)?;
        files.retain(|f| f != "SKILL.md");
        files.sort();
        Ok(SkillRecord { meta, body, files })
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

    fn load_usage(&self) -> Result<UsageLog, SkillError> {
        match std::fs::read_to_string(self.usage_path()) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|e| SkillError::Storage(format!("corrupt .usage.json: {e}"))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(UsageLog::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn save_usage(&self, log: &UsageLog) -> Result<(), SkillError> {
        let raw = serde_json::to_string_pretty(log)
            .map_err(|e| SkillError::Storage(e.to_string()))?;
        Ok(std::fs::write(self.usage_path(), raw)?)
    }
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
