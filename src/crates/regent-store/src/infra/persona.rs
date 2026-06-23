//! Persona persistence — the agent's soul + the user's profile, stored in the
//! DB (not plaintext files under $REGENT_HOME) for security. Seeded empty on
//! open so both always exist + are editable via `regent soul` / `regent about`.

use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::{OptionalExtension, params};

/// The user profile (semantic memory of kind `persona`/`preference`, per the
/// architecture proposal §5.3) is split into five stable facets. Each is a
/// persona row keyed `about.<slug>`; transient/world facts go to memory, not
/// here. Order = render order.
pub const ABOUT_SECTIONS: [(&str, &str); 5] = [
    ("identity", "Identity"),
    ("preferences", "Preferences"),
    ("habits", "Habits"),
    ("constraints", "Constraints"),
    ("goals", "Goals"),
];

/// True for a persona key the CLI/tool/RPC may read or write:
/// `soul`, `about` (legacy general note), or `about.<one of the five facets>`.
#[must_use]
pub fn is_valid_persona_key(key: &str) -> bool {
    if key == "soul" || key == "about" {
        return true;
    }
    key.strip_prefix("about.")
        .is_some_and(|s| ABOUT_SECTIONS.iter().any(|(slug, _)| *slug == s))
}

impl Store {
    /// Seed empty `soul`/`about` (+ the five `about.<facet>`) rows so the
    /// persona always exists + is editable.
    pub fn seed_persona(&self) -> Result<(), StoreError> {
        self.with_write(|tx| {
            for key in ["soul", "about"] {
                tx.execute(
                    "INSERT OR IGNORE INTO persona (key, content, updated_at) VALUES (?1, '', ?2)",
                    params![key, now_epoch()],
                )?;
            }
            for (slug, _) in ABOUT_SECTIONS {
                tx.execute(
                    "INSERT OR IGNORE INTO persona (key, content, updated_at) VALUES (?1, '', ?2)",
                    params![format!("about.{slug}"), now_epoch()],
                )?;
            }
            Ok(())
        })
    }

    /// Persona content for `key` (`soul` | `about`); "" when unset.
    pub fn get_persona(&self, key: &str) -> Result<String, StoreError> {
        self.with_read(|conn| {
            conn.query_row("SELECT content FROM persona WHERE key = ?1", params![key], |r| {
                r.get::<_, String>(0)
            })
            .optional()
        })
        .map(Option::unwrap_or_default)
    }

    /// Upsert persona content for `key`.
    pub fn set_persona(&self, key: &str, content: &str) -> Result<(), StoreError> {
        self.with_write(|tx| {
            tx.execute(
                "INSERT INTO persona (key, content, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET content = ?2, updated_at = ?3",
                params![key, content, now_epoch()],
            )?;
            Ok(())
        })
    }

    /// The persona prompt block (soul + about), or "" when both are empty.
    /// Injected into the system prompt by the daemon and the gateway.
    #[must_use]
    pub fn persona_block(&self) -> String {
        let mut out = String::new();
        let soul = self.get_persona("soul").unwrap_or_default();
        if !soul.trim().is_empty() {
            out.push_str(
                "\n\n## Your persona — this overrides the default tone/identity when they differ\n",
            );
            out.push_str(soul.trim());
        }
        // The user profile: a legacy free-text note (back-compat) plus the five
        // structured facets. Header is emitted once, only if something's there.
        let legacy = self.get_persona("about").unwrap_or_default();
        let facets: Vec<(&str, String)> = ABOUT_SECTIONS
            .iter()
            .filter_map(|(slug, heading)| {
                let v = self.get_persona(&format!("about.{slug}")).unwrap_or_default();
                (!v.trim().is_empty()).then(|| (*heading, v.trim().to_owned()))
            })
            .collect();
        if !legacy.trim().is_empty() || !facets.is_empty() {
            out.push_str("\n\n## About the person you're helping\n");
            if !legacy.trim().is_empty() {
                out.push_str(legacy.trim());
            }
            for (heading, content) in facets {
                out.push_str(&format!("\n\n### {heading}\n{content}"));
            }
        }
        out
    }

    /// One-time migration: import a legacy `soul.md` / `about-you.md` under
    /// `home` into the DB (when the row is still empty), then delete the file —
    /// persona is DB-only now. Best-effort; missing files are a no-op.
    pub fn import_persona_files(&self, home: &str) {
        for (file, key) in [("soul.md", "soul"), ("about-you.md", "about")] {
            let path = std::path::Path::new(home).join(file);
            if let Ok(content) = std::fs::read_to_string(&path) {
                if !content.trim().is_empty()
                    && self.get_persona(key).unwrap_or_default().trim().is_empty()
                {
                    let _ = self.set_persona(key, content.trim());
                }
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}
