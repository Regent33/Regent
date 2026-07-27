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
//!
//! ## The starvation guard
//!
//! The skills index caps at 24 by recency, and appearing in it does **not**
//! bump `last_activity_at`. So a skill below the cut is never shown, never
//! used, never refreshed, and ages into the archive branch for an idleness the
//! cap created. `plan_curation` answers the question directly — was this skill
//! visible? — instead of approximating it. A reserved-slot floor in the index
//! was tried first and does not work: it guarantees exposure to a fixed four
//! forever (exposure never refreshes them), leaves the rest of the tail
//! starving, and evicts genuinely recent skills to do it [co-audit].
//!
//! **What the guard does not promise** [co-audit]. Precisely: no skill is
//! archived while absent from the visible set computed in the same snapshot.
//! That is narrower than "never archived for idleness the cap caused" — a
//! skill hidden for 89 of its 90 idle days, then promoted by another skill's
//! archival, is visible when judged and archived on the spot. It also binds
//! only `curate`; `SkillLibrary::archive` called directly is an explicit
//! decision and deliberately unguarded.
//!
//! It asks whether a skill was visible
//! *at the moment of judgment*, not whether it had a fair window. Archiving
//! the visible long-idle skills promotes the next tranche into the index, so a
//! deep tail still drains a batch per pass — each skill visible for as little
//! as one interval before its turn. A real minimum-exposure guarantee needs
//! `last_exposed_at` persisted separately from `last_activity_at` (folding it
//! into the latter would make mere exposure outrank actual use in the MRU
//! ranking), and that is an owner decision, not a curator one. Latent for now:
//! the cap needs 24 skills to bite and the live library holds 13.

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
    /// Old enough to archive, but the skills index is not showing it — so its
    /// idleness is the cap's doing, not a verdict on the skill. Reported,
    /// never applied. See the starvation note below.
    HiddenByIndexCap,
}

impl CurationAction {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::MarkStale => "mark_stale",
            Self::Archive => "archive",
            Self::Untracked => "untracked",
            Self::HiddenByIndexCap => "hidden_by_index_cap",
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
    // ONE snapshot for everything: the candidate set, the idle clock, and the
    // visibility test all come from the same `list()` + `load_usage()` pair.
    // Reading them separately let one plan judge a skill's age against usage A
    // while ranking its visibility against usage C [co-audit] — and cost a
    // second full record pass every 6 hours to do it.
    let summaries = library.list()?;
    let usage = library.repository().load_usage()?;
    let visible = SkillLibrary::visible_from(&summaries, &usage);
    let mut out = Vec::new();

    for summary in summaries {
        // Hard scope: agent-created and unpinned. A user's or bundled skill is
        // never a curation candidate, not even as a suggestion.
        if !summary.curatable {
            continue;
        }
        let name = summary.name;
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
            // The starvation guard. The skills index caps at 24 by recency, and
            // being listed does NOT bump `last_activity_at` — so a skill below
            // the cut is never shown, never used, never refreshed, and ages
            // straight into this branch for an idleness the cap itself caused.
            //
            // Judged where it is exact rather than where it is convenient: a
            // reserved-slot "floor" in the index looked like the fix and is not
            // [co-audit]. With 50 skills it shows ranks 1-20 and 47-50 — the
            // same four hold the reserved slots forever, since exposure never
            // refreshes them, while 21-46 starve exactly as before and 21-24
            // LOSE the visibility they had. Here the question is answered
            // directly: was the model in a position to use this skill?
            if visible.contains(&name) {
                CurationAction::Archive
            } else {
                CurationAction::HiddenByIndexCap
            }
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
