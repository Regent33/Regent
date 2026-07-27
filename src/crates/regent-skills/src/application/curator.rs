//! Curator use case — skill lifecycle maintenance over usage telemetry.
//! Invariants: only `created_by: agent` skills are
//! touched; pinned skills are exempt from every transition; the most
//! destructive action is archive — never delete.
//!
//! ## W5: what the telemetry can and cannot support (measured 2026-07-28)
//!
//! The plan's W5 row said `.usage.json` "records `use_count`/`view_count`/
//! `state` and nothing consumes it". Both halves are wrong, and the correction
//! matters more than the row did:
//!
//! - **Two things already consume it**, automatically: this curator (every 6h
//!   via `spawn_curator`) and the skills-index MRU cap (`index_render`, past 24
//!   skills). The plan's own gate says automatic ranking waits for W1 efficacy
//!   attribution. Ranking has been live the whole time.
//! - **`use_count` is dead outside a dev REPL.** `record_use` has exactly one
//!   caller, `regent-agent/src/bin/repl.rs`. Nothing in the deacon path — CLI,
//!   Desktop, gateway, voice — ever bumps it. The live library confirms it:
//!   13 skills, `use_count` 0 on every one.
//! - **A skill with no telemetry row is invisible to curation forever**, since
//!   the loop below skips it. 12 of the owner's 13 skills have no row (they
//!   predate `create` recording one). So the automatic pass can act on exactly
//!   one skill in thirteen.
//!
//! Hence [`plan_curation`]: it reports on the **whole** library, including the
//! skills the automatic pass cannot see, and writes nothing. `curate` still
//! applies only the conservative subset it always did — widening it would
//! silently archive a dozen of the owner's skills, which is the owner's call,
//! not the curator's.

use crate::application::library::SkillLibrary;
use crate::domain::entities::SkillState;
use crate::domain::errors::SkillError;

#[derive(Debug, Clone)]
pub struct CuratorConfig {
    pub stale_after_days: f64,
    pub archive_after_days: f64,
}

impl Default for CuratorConfig {
    fn default() -> Self {
        Self {
            stale_after_days: 30.0,
            archive_after_days: 90.0,
        }
    }
}

/// What curation would do to one skill. `Untracked` is not an action — it is
/// the absence of one, reported because silence and "nothing to do" look
/// identical from outside and mean opposite things.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurationAction {
    MarkStale,
    Archive,
    Untracked,
}

impl CurationAction {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::MarkStale => "mark_stale",
            Self::Archive => "archive",
            Self::Untracked => "untracked",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub name: String,
    pub action: CurationAction,
    /// Days since last recorded activity; `None` when there is no telemetry
    /// row, which is exactly the case the automatic pass cannot see.
    pub idle_days: Option<f64>,
}

#[derive(Debug, Default)]
pub struct CuratorReport {
    pub marked_stale: Vec<String>,
    pub archived: Vec<String>,
}

/// **Suggestion only — writes nothing.** What curation would do across the
/// whole library, including skills the automatic pass skips.
pub fn plan_curation(
    library: &SkillLibrary,
    now_epoch: f64,
    config: &CuratorConfig,
) -> Result<Vec<Suggestion>, SkillError> {
    let repository = library.repository();
    let usage = repository.load_usage()?;
    let mut out = Vec::new();

    for name in repository.list_names()? {
        let record = match repository.load(&name) {
            Ok(record) => record,
            Err(error) => {
                tracing::warn!(skill = name, %error, "curator skipping unreadable skill");
                continue;
            }
        };
        // Hard scope: agent-created and unpinned. A user's skill is never a
        // curation candidate, not even as a suggestion.
        if record.meta.created_by != "agent" || record.meta.pinned {
            continue;
        }
        let Some(telemetry) = usage.skills.get(&name) else {
            out.push(Suggestion {
                name,
                action: CurationAction::Untracked,
                idle_days: None,
            });
            continue;
        };
        let idle_days = (now_epoch - telemetry.last_activity_at).max(0.0) / 86_400.0;
        let action = if idle_days >= config.archive_after_days {
            CurationAction::Archive
        } else if idle_days >= config.stale_after_days && telemetry.state == SkillState::Active {
            CurationAction::MarkStale
        } else {
            continue;
        };
        out.push(Suggestion {
            name,
            action,
            idle_days: Some(idle_days),
        });
    }
    Ok(out)
}

/// Applies the transitions the curator is allowed to make on its own. Scoped to
/// skills that HAVE telemetry — see the module note: widening it to untracked
/// skills is a behavior change on a live library, and belongs to whoever owns
/// that library.
pub fn curate(
    library: &SkillLibrary,
    now_epoch: f64,
    config: &CuratorConfig,
) -> Result<CuratorReport, SkillError> {
    let repository = library.repository();
    let mut report = CuratorReport::default();
    let suggestions = plan_curation(library, now_epoch, config)?;
    if suggestions.is_empty() {
        return Ok(report);
    }
    let mut usage = repository.load_usage()?;

    for suggestion in suggestions {
        // Preserve the original clock: these transitions are bookkeeping, and
        // stamping `now` would reset the idle timer the next pass reads.
        let Some(last) = usage
            .skills
            .get(&suggestion.name)
            .map(|r| r.last_activity_at)
        else {
            continue;
        };
        match suggestion.action {
            CurationAction::Archive => {
                repository.archive(&suggestion.name)?;
                usage.touch(&suggestion.name, last, |r| r.state = SkillState::Archived);
                report.archived.push(suggestion.name);
            }
            CurationAction::MarkStale => {
                usage.touch(&suggestion.name, last, |r| r.state = SkillState::Stale);
                report.marked_stale.push(suggestion.name);
            }
            CurationAction::Untracked => {}
        }
    }

    repository.save_usage(&usage)?;
    Ok(report)
}
