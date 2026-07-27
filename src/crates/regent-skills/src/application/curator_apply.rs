//! Applying a curation plan. Split from `curator.rs` (file-size rule);
//! the plan/apply seam is real, not cosmetic — a plan can be reviewed,
//! logged or handed to an owner before anything is written.

use super::curator::{CurationAction, CuratorConfig, CuratorReport, Suggestion, plan_curation};
use crate::application::library::SkillLibrary;
use crate::domain::entities::SkillState;
use crate::domain::errors::SkillError;

/// Applies the transitions the curator is allowed to make on its own. Scoped to
/// skills that HAVE telemetry — see the module note: widening it to untracked
/// skills is a behavior change on a live library, and belongs to whoever owns
/// that library.
pub fn curate(
    library: &SkillLibrary,
    now_epoch: f64,
    config: &CuratorConfig,
) -> Result<CuratorReport, SkillError> {
    let plan = plan_curation(library, now_epoch, config)?;
    apply_plan(library, plan, now_epoch, config)
}

/// Applies a plan. Split from [`plan_curation`] so a plan can be reviewed
/// before it is acted on — and so the revalidation below is reachable by a
/// test rather than only by a race.
pub fn apply_plan(
    library: &SkillLibrary,
    plan: Vec<Suggestion>,
    now_epoch: f64,
    config: &CuratorConfig,
) -> Result<CuratorReport, SkillError> {
    let repository = library.repository();
    let mut report = CuratorReport::default();
    if plan.is_empty() {
        return Ok(report);
    }
    let mut usage = repository.load_usage()?;

    for suggestion in plan {
        // Preserve the original clock: these transitions are bookkeeping, and
        // stamping `now` would reset the idle timer the next pass reads.
        let Some(last) = usage
            .skills
            .get(&suggestion.name)
            .map(|r| r.last_activity_at)
        else {
            continue;
        };
        // Revalidate against telemetry loaded AFTER the plan. A skill used
        // between planning and applying has just proved itself, and retiring it
        // on a stale timestamp is the one race here with a user-visible cost
        // [co-audit]. Free — `usage` is already in hand.
        let threshold = match suggestion.action {
            CurationAction::Archive => config.archive_after_days,
            CurationAction::MarkStale => config.stale_after_days,
            CurationAction::Untracked | CurationAction::HiddenByIndexCap => continue,
        };
        if (now_epoch - last).max(0.0) / 86_400.0 < threshold {
            continue;
        }
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
            // Report-only outcomes. Neither is a decision the curator is in a
            // position to make: one has no clock, the other has no evidence.
            CurationAction::Untracked | CurationAction::HiddenByIndexCap => {}
        }
    }

    repository.save_usage(&usage)?;
    Ok(report)
}
