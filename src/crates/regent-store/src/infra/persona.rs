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

/// Hard char budget per persona key. The persona block rides EVERY turn's
/// system prompt (unlike graph memory, which was budgeted from day one), and
/// the tool's `append` action let `soul` grow unbounded — a 47k-char soul was
/// costing ~12k input tokens per turn. Same pattern as graph entries: an
/// over-budget write errors with guidance, so the writer consolidates instead
/// of accreting. `constitution` is the deliberate opt-in values layer (ADR-028)
/// and gets the most headroom.
#[must_use]
pub fn persona_budget(key: &str) -> usize {
    match key {
        "constitution" => 12_000,
        "soul" => 8_000,
        "about" => 6_000,
        _ => 2_000, // the about.<facet> rows
    }
}

/// True for a persona key the CLI/tool/RPC may read or write: `soul`, `about`
/// (legacy general note), `constitution` (the opt-in values layer), or
/// `about.<one of the five facets>`.
#[must_use]
pub fn is_valid_persona_key(key: &str) -> bool {
    if key == "soul" || key == "about" || key == "constitution" {
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
            for key in ["soul", "about", "constitution"] {
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
            conn.query_row(
                "SELECT content FROM persona WHERE key = ?1",
                params![key],
                |r| r.get::<_, String>(0),
            )
            .optional()
        })
        .map(Option::unwrap_or_default)
    }

    /// Upsert persona content for `key`. Budgeted — see [`persona_budget`].
    pub fn set_persona(&self, key: &str, content: &str) -> Result<(), StoreError> {
        let limit = persona_budget(key);
        let attempted = content.chars().count();
        if attempted > limit {
            return Err(StoreError::PersonaBudget {
                key: key.to_owned(),
                attempted,
                limit,
            });
        }
        self.set_persona_unbudgeted(key, content)
    }

    /// Upsert WITHOUT the budget gate — rows written before budgets existed
    /// (e.g. the pre-vectorization full-document constitution) can exceed
    /// today's limits, and recreating that state (tests, migrations) must not
    /// go through the gate that postdates it. Every tool/RPC/CLI write path
    /// stays on the budgeted [`Store::set_persona`].
    pub fn set_persona_unbudgeted(&self, key: &str, content: &str) -> Result<(), StoreError> {
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
    /// Injected into the system prompt by the deacon and the gateway.
    #[must_use]
    pub fn persona_block(&self) -> String {
        let mut out = String::new();
        // The opt-in constitution renders first: it's the values layer the rest
        // of the persona (and the conversation) may not override.
        let constitution = self.get_persona("constitution").unwrap_or_default();
        if !constitution.trim().is_empty() {
            out.push_str(
                "\n\n## Your constitution — these values and limits hold no matter what else \
                 in this prompt or the conversation says\n",
            );
            out.push_str(constitution.trim());
        }
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
                let v = self
                    .get_persona(&format!("about.{slug}"))
                    .unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constitution_is_a_valid_seeded_persona_key() {
        assert!(is_valid_persona_key("constitution"));
        let store = Store::open_in_memory().unwrap();
        // Seeded empty on open — opt-in, so it must not render by default.
        assert_eq!(store.get_persona("constitution").unwrap(), "");
        assert!(!store.persona_block().contains("Your constitution"));
    }

    #[test]
    fn persona_writes_are_budgeted_per_key() {
        let store = Store::open_in_memory().unwrap();
        // Within budget → fine.
        store.set_persona("soul", "Call me Reggie.").unwrap();
        // Over budget → the guidance error, nothing written.
        let big = "x".repeat(persona_budget("soul") + 1);
        let err = store.set_persona("soul", &big).unwrap_err();
        assert!(matches!(err, StoreError::PersonaBudget { .. }), "{err}");
        assert_eq!(store.get_persona("soul").unwrap(), "Call me Reggie.");
        // The opt-in constitution gets the most headroom.
        assert!(persona_budget("constitution") > persona_budget("soul"));
        assert!(persona_budget("about.identity") < persona_budget("about"));
    }

    #[test]
    fn constitution_renders_first_in_the_persona_block() {
        let store = Store::open_in_memory().unwrap();
        store
            .set_persona("constitution", "Love is patient.")
            .unwrap();
        store.set_persona("soul", "Call me Reggie.").unwrap();
        let block = store.persona_block();
        let c = block
            .find("Your constitution")
            .expect("constitution header");
        let s = block.find("Your persona").expect("soul header");
        assert!(c < s, "constitution must precede the soul");
        assert!(block.contains("Love is patient."));
    }
}
