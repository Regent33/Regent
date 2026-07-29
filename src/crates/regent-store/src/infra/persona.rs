//! Persona persistence — the agent's soul + the user's profile, stored in the
//! DB (not plaintext files under $REGENT_HOME) for security. Seeded empty on
//! open so both always exist + are editable via `regent soul` / `regent about`.

use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::params;

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

/// The soul a fresh install wakes up with — INSERT OR IGNORE means only a
/// truly new DB gets it; an existing row (even one the user blanked) is never
/// overwritten. A token-lean distillation of SYSTEM_PROMPT's own character
/// (same voice, same rules of bearing — regent-agent prompts/system.rs);
/// mechanics live in the system prompt, this is identity only. Short on
/// purpose: it rides every turn's system prompt.
pub const DEFAULT_SOUL: &str = "You are Regent — a kind, thoughtful, warm, and capable AI \
agent. Built to serve.\n\
- Genuinely care about the person you're helping: notice how they're doing, celebrate their \
wins — a few well-placed emojis (1-3, never walls).\n\
- Concise and direct: match reply length to the ask; act with your tools instead of padding.\n\
- Do exactly what's asked and no more — the simplest path that fully answers; go deeper only \
when invited.\n\
- When you get something wrong, own it plainly and fix it — never argue with a correction.\n\
- If you don't know, say so — then offer to find out.";

impl Store {
    /// Every persona key seeding must guarantee exists.
    fn seeded_keys() -> Vec<String> {
        let mut keys = vec![
            "soul".to_owned(),
            "about".to_owned(),
            "constitution".to_owned(),
        ];
        keys.extend(
            ABOUT_SECTIONS
                .iter()
                .map(|(slug, _)| format!("about.{slug}")),
        );
        keys
    }

    /// Whether seeding would be a no-op: every key present, and `soul`
    /// non-empty (the backfill case below).
    ///
    /// One read, so the common path never opens a write transaction. Racing
    /// another process is safe: a partial read just falls through to the write,
    /// which is idempotent.
    fn persona_seed_complete(&self) -> Result<bool, StoreError> {
        let keys = Self::seeded_keys();
        let holes = "?,".repeat(keys.len());
        let sql = format!(
            "SELECT COUNT(*), COALESCE(MAX(key = 'soul' AND content <> ''), 0) \
             FROM persona WHERE key IN ({})",
            holes.trim_end_matches(',')
        );
        let params = rusqlite::params_from_iter(keys.iter());
        let (present, soul_ok): (i64, i64) =
            self.with_read(|conn| conn.query_row(&sql, params, |r| Ok((r.get(0)?, r.get(1)?))))?;
        Ok(present == keys.len() as i64 && soul_ok == 1)
    }

    /// Seed the persona rows so they always exist + are editable: `soul`
    /// starts as [`DEFAULT_SOUL`] (fresh installs shouldn't wake up
    /// personality-less), everything else starts empty.
    ///
    /// Called on EVERY `Store::open`, which is every deacon boot and — since
    /// the CLI spawns a deacon per command — every CLI invocation, and it used
    /// to take a `BEGIN IMMEDIATE` write transaction unconditionally even when
    /// every row already existed.
    ///
    /// Skipping that write is worth **~0.14 ms of a 2.87 ms open** — measured,
    /// after guessing higher and being wrong (see `open_cost.rs`). It is kept
    /// because an unconditional write transaction per open is the wrong shape
    /// regardless: with several sessions live at once it is pure WAL churn and
    /// lock contention for a no-op. It is not kept for the 0.14 ms.
    pub fn seed_persona(&self) -> Result<(), StoreError> {
        if self.persona_seed_complete()? {
            return Ok(());
        }
        self.with_write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO persona (key, content, updated_at) VALUES ('soul', ?1, ?2)",
                params![DEFAULT_SOUL, now_epoch()],
            )?;
            // Installs that predate the soul feature carry an EXISTING empty
            // soul row, which INSERT OR IGNORE skips forever — the owner's own
            // install woke up personality-less this way. An empty soul is
            // never a deliberate state (personality is a product invariant,
            // like the constitution), so backfill it.
            tx.execute(
                "UPDATE persona SET content = ?1, updated_at = ?2 \
                 WHERE key = 'soul' AND content = ''",
                params![DEFAULT_SOUL, now_epoch()],
            )?;
            for key in ["about", "constitution"] {
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

    /// Persona content for `key` (`soul` | `about`); "" when unset. Reads the
    /// ACTIVE profile's row (see `infra::profiles`) — the constitution and
    /// internal keys resolve to themselves.
    pub fn get_persona(&self, key: &str) -> Result<String, StoreError> {
        self.raw_persona(&self.resolve_persona_key(key))
    }

    /// Upsert persona content for `key`. Budgeted — see [`persona_budget`] —
    /// and written to the ACTIVE profile's row (budget checks the user-facing
    /// key, storage lands on the resolved one).
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
        self.set_raw_persona(&self.resolve_persona_key(key), content)
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
#[path = "persona_tests.rs"]
mod tests;
