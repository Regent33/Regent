//! Profiles — named persona namespaces over the same `persona` table. The
//! active profile's `soul`/`about.*` rows feed every prompt; `default` maps
//! to the bare legacy keys, so existing installs keep their persona untouched.
//! The constitution (ADR-028) is deliberately GLOBAL: switching profiles can
//! never swap the values layer.

use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use crate::infra::persona::is_valid_persona_key;
use rusqlite::{OptionalExtension, params};

/// The row holding the active profile's name ("" / missing = `default`).
const ACTIVE_KEY: &str = "profile.active";
pub const DEFAULT_PROFILE: &str = "default";

/// Profile names are slugs: 1–32 chars of `a-z 0-9 -`. Keeps the namespaced
/// persona keys (`profile.<name>.soul`) unambiguous to parse back.
#[must_use]
pub fn is_valid_profile_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Store {
    /// The active profile's name; `default` when never switched.
    #[must_use]
    pub fn active_profile(&self) -> String {
        let name = self.raw_persona(ACTIVE_KEY).unwrap_or_default();
        if name.is_empty() {
            DEFAULT_PROFILE.to_owned()
        } else {
            name
        }
    }

    /// Every profile name: `default` first (always present), then the named
    /// ones alphabetically (a profile exists iff its `.soul` row does).
    pub fn list_profiles(&self) -> Result<Vec<String>, StoreError> {
        let mut names = self.with_read(|conn| {
            let mut stmt =
                conn.prepare("SELECT key FROM persona WHERE key LIKE 'profile.%.soul'")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()
        })?;
        names.sort();
        let mut out = vec![DEFAULT_PROFILE.to_owned()];
        out.extend(names.iter().filter_map(|k| {
            k.strip_prefix("profile.")?
                .strip_suffix(".soul")
                .map(str::to_owned)
        }));
        Ok(out)
    }

    /// Creates an empty named profile. Rejects bad slugs and existing names.
    pub fn create_profile(&self, name: &str) -> Result<(), StoreError> {
        if !is_valid_profile_name(name) {
            return Err(StoreError::Profile(format!(
                "'{name}' — profile names are 1-32 chars of a-z, 0-9, '-'"
            )));
        }
        if self.list_profiles()?.iter().any(|p| p == name) {
            return Err(StoreError::Profile(format!("'{name}' already exists")));
        }
        // The `.soul` row IS the profile's existence marker.
        self.set_raw_persona(&format!("profile.{name}.soul"), "")
    }

    /// Makes `name` the active profile (must exist). Takes effect for prompts
    /// built after the switch — running sessions keep their frozen prompt.
    pub fn switch_profile(&self, name: &str) -> Result<(), StoreError> {
        if !self.list_profiles()?.iter().any(|p| p == name) {
            return Err(StoreError::Profile(format!(
                "no profile named '{name}' — create it first"
            )));
        }
        self.set_raw_persona(ACTIVE_KEY, name)
    }

    /// Maps a user-facing persona key (`soul`, `about.*`) onto the active
    /// profile's storage key. The constitution and internal/unknown keys pass
    /// through untouched; `default` uses the bare legacy keys.
    pub(crate) fn resolve_persona_key(&self, key: &str) -> String {
        if key == "constitution" || !is_valid_persona_key(key) {
            return key.to_owned();
        }
        match self.active_profile() {
            name if name == DEFAULT_PROFILE => key.to_owned(),
            name => format!("profile.{name}.{key}"),
        }
    }

    /// Raw row read — no profile resolution (internal bookkeeping keys).
    pub(crate) fn raw_persona(&self, key: &str) -> Result<String, StoreError> {
        self.with_read(|conn| {
            conn.query_row(
                "SELECT content FROM persona WHERE key = ?1",
                params![key],
                |r| r.get::<_, String>(0),
            )
            .optional()
        })
        .map(Option::unwrap_or_default)
    }

    /// Raw row upsert — no profile resolution, no budget (internal keys).
    pub(crate) fn set_raw_persona(&self, key: &str, content: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO persona (key, content, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET content = ?2, updated_at = ?3",
                params![key, content, now_epoch()],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
