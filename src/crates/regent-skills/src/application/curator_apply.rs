//! Applying a curation plan. Split from `curator.rs` (file-size rule);
//! the plan/apply seam is real, not cosmetic — a plan can be reviewed,
//! logged or handed to an owner before anything is written.

use super::curator::{CurationAction, CuratorConfig, CuratorReport, Suggestion, plan_curation};
use crate::application::library::SkillLibrary;
use crate::domain::entities::SkillState;
use crate::domain::errors::SkillError;

/// Runs a full pass: plan, maintain the exposure clock, apply what is allowed.
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
///
/// Two things happen here that are not "transitions", and both are why this
/// writes even when nothing is archived:
///
/// - **Adoption.** A skill with no telemetry row gets one. Until this, 12 of
///   the owner's 13 skills were invisible to curation forever, and the obvious
///   alternative — treating a missing row as infinite idleness — would have
///   archived skills created *days* ago. The clock starts now, because when it
///   was last used is genuinely unknown and inventing a date is worse than
///   admitting that.
/// - **The exposure clock.** `visible_since` is set the first pass a skill is
///   visible and cleared the moment it is not, so idleness only counts against
///   a skill that could actually be reached.
pub fn apply_plan(
    library: &SkillLibrary,
    plan: Vec<Suggestion>,
    now_epoch: f64,
    config: &CuratorConfig,
) -> Result<CuratorReport, SkillError> {
    let repository = library.repository();
    let mut report = CuratorReport::default();
    let mut usage = repository.load_usage()?;

    // Adopt first: an untracked skill has no row for the clock below to touch.
    for suggestion in plan
        .iter()
        .filter(|s| s.action == CurationAction::Untracked)
    {
        usage.touch(&suggestion.name, now_epoch, |_| {});
    }

    // The exposure clock, over the library as it stands after adoption.
    //
    // Written through `get_mut`, never `touch`: `touch` stamps
    // `last_activity_at`, and applying it here would reset the idle clock of
    // every skill in the library on every pass — nothing would ever age.
    let summaries = library.list()?;
    let visible = SkillLibrary::visible_from(&summaries, &usage);
    for summary in summaries.iter().filter(|s| s.curatable) {
        let seen = visible.contains(&summary.name);
        if let Some(record) = usage.skills.get_mut(&summary.name) {
            // Continuity matters: a skill that drops out of the index and comes
            // back starts over, because what it needs is a *window* of
            // reachability, not a running total of moments.
            record.visible_since = match (seen, record.visible_since) {
                (true, Some(since)) => Some(since),
                (true, None) => Some(now_epoch),
                (false, _) => None,
            };
        }
    }

    for suggestion in plan {
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
            // Report-only outcomes. None is a decision the curator is in a
            // position to make: one has no clock, one has no evidence, one has
            // not had a fair chance yet.
            CurationAction::Untracked
            | CurationAction::HiddenByIndexCap
            | CurationAction::AwaitingExposure => continue,
        };
        if (now_epoch - last).max(0.0) / 86_400.0 < threshold {
            continue;
        }
        // Preserve the original clock: these transitions are bookkeeping, and
        // stamping `now` would reset the idle timer the next pass reads.
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
            _ => {}
        }
    }

    repository.save_usage(&usage)?;
    Ok(report)
}
