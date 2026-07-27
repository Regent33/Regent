//! W5: the starvation guard and the plan/apply seam. Split from
//! `curator.rs` (file-size rule).

use crate::{library, now as wall_clock};
use regent_skills::{
    CurationAction, CuratorConfig, FsSkillRepository, SkillRepository, SkillState, apply_plan,
    curate, plan_curation,
};

/// W5 starvation guard: the skills index caps at 24 by recency, and being
/// listed never bumps `last_activity_at`. So a skill below the cut cannot be
/// used, cannot refresh, and ages into the archive branch for an idleness the
/// cap itself caused. It must be reported, not archived.
#[test]
fn a_skill_the_index_cap_is_hiding_is_never_archived_for_being_idle() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    // 40 disk skills against an index cap of 24: some are necessarily hidden.
    for i in 0..40 {
        lib.create(
            &format!("skill-{i:02}"),
            "Some fine description.",
            "b",
            "agent",
        )
        .unwrap();
    }
    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let now = 1_000_000_000.0;
    let mut usage = repo.load_usage().unwrap();
    // Every one of them well past the 90-day archive threshold.
    for i in 0..40 {
        usage
            .skills
            .get_mut(&format!("skill-{i:02}"))
            .unwrap()
            .last_activity_at = now - 200.0 * 86_400.0 - i as f64;
    }
    repo.save_usage(&usage).unwrap();

    let visible = lib.index_visible_names().unwrap();
    let plan = plan_curation(&lib, now, &CuratorConfig::default()).unwrap();
    let hidden: Vec<&str> = plan
        .iter()
        .filter(|s| s.action == CurationAction::HiddenByIndexCap)
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !hidden.is_empty(),
        "the cap hides skills here, so some must be reported rather than archived"
    );
    for name in &hidden {
        assert!(!visible.contains(*name), "{name} is visible, not hidden");
    }

    let report = curate(&lib, now, &CuratorConfig::default()).unwrap();
    for name in &hidden {
        assert!(
            !report.archived.contains(&(*name).to_owned()),
            "{name} was archived for idleness the index cap caused"
        );
        assert!(repo.list_names().unwrap().contains(&(*name).to_owned()));
    }
    // The guard is narrow, not a blanket amnesty: what the model COULD see and
    // still did not use for 200 days is archived as before.
    assert!(
        !report.archived.is_empty(),
        "visible, long-idle skills must still archive"
    );
    for name in report.archived {
        assert!(visible.contains(&name), "{name} was archived while hidden");
    }
}

/// [co-audit] `plan_curation` reads, then `apply_plan` writes. In between, a
/// live session can view the very skill the plan condemned — and that view is
/// the strongest possible evidence the skill is still wanted. Applying a stale
/// plan must not retire it. Reachable here because plan and apply are separate
/// calls; in production the window is the gap between the two reads.
#[test]
fn a_plan_is_revalidated_so_a_skill_used_since_planning_survives() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create(
        "reprieved",
        "Agent skill about to be archived.",
        "body",
        "agent",
    )
    .unwrap();
    lib.create("doomed", "Agent skill nobody wants.", "body", "agent")
        .unwrap();

    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let now = wall_clock();
    let mut usage = repo.load_usage().unwrap();
    for name in ["reprieved", "doomed"] {
        usage.skills.get_mut(name).unwrap().last_activity_at = now - 200.0 * 86_400.0;
    }
    repo.save_usage(&usage).unwrap();

    let plan = plan_curation(&lib, now, &CuratorConfig::default()).unwrap();
    assert_eq!(plan.len(), 2, "both are archive candidates when planned");
    assert!(plan.iter().all(|s| s.action == CurationAction::Archive));

    // ...and now someone actually uses one of them.
    lib.view("reprieved").unwrap();

    let report = apply_plan(&lib, plan, now, &CuratorConfig::default()).unwrap();
    assert_eq!(
        report.archived,
        vec!["doomed"],
        "a skill used between planning and applying must survive the stale plan"
    );
    assert!(repo.list_names().unwrap().contains(&"reprieved".to_owned()));
}

/// The guard must not fire when nothing is hidden, or dead skills accumulate
/// forever under a cap that never bit [co-audit].
#[test]
fn under_the_index_cap_nothing_is_ever_reported_as_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    for i in 0..3 {
        lib.create(&format!("s-{i}"), "Some fine description.", "b", "agent")
            .unwrap();
    }
    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let now = 1_000_000_000.0;
    let mut usage = repo.load_usage().unwrap();
    for i in 0..3 {
        usage
            .skills
            .get_mut(&format!("s-{i}"))
            .unwrap()
            .last_activity_at = now - 200.0 * 86_400.0;
    }
    repo.save_usage(&usage).unwrap();

    let plan = plan_curation(&lib, now, &CuratorConfig::default()).unwrap();
    assert!(
        !plan
            .iter()
            .any(|s| s.action == CurationAction::HiddenByIndexCap),
        "3 skills against a cap of 24 hides nothing: {plan:?}"
    );
    assert_eq!(
        curate(&lib, now, &CuratorConfig::default())
            .unwrap()
            .archived
            .len(),
        3
    );
}

/// The curator's visibility test and the index the model actually reads must
/// agree. They share a helper, so compare against the RENDERED text — a bug in
/// that helper would otherwise make both wrong in the same direction and pass
/// every other test here [co-audit].
#[test]
fn the_curators_visible_set_matches_the_rendered_index() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    for i in 0..40 {
        lib.create(
            &format!("skill-{i:02}"),
            "Some fine description.",
            "b",
            "agent",
        )
        .unwrap();
    }
    // Distinct timestamps so the ranking is not decided entirely by tie-break.
    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let now = 1_000_000_000.0;
    let mut usage = repo.load_usage().unwrap();
    for i in 0..40 {
        usage
            .skills
            .get_mut(&format!("skill-{i:02}"))
            .unwrap()
            .last_activity_at = now - (i as f64) * 86_400.0;
    }
    repo.save_usage(&usage).unwrap();

    let index = lib.render_index().unwrap();
    let visible = lib.index_visible_names().unwrap();
    for name in &visible {
        assert!(
            index.contains(&format!("- {name}:")),
            "{name} counts as visible but is not in the rendered index"
        );
    }
    let rendered = index.matches("\n- ").count() - 1; // less the overflow pointer
    assert_eq!(
        rendered,
        visible.len(),
        "the rendered index and the curator's visible set must be the same size"
    );
}

/// The transitions are bookkeeping: stamping `now` would reset the idle clock
/// the next pass reads, so a skill marked stale at 40 days would look 0 days
/// idle and never reach the archive threshold.
#[test]
fn marking_stale_does_not_reset_the_idle_clock() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create("drifting", "Agent skill going stale.", "body", "agent")
        .unwrap();

    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let now = 1_000_000_000.0;
    let stamped = now - 40.0 * 86_400.0;
    let mut usage = repo.load_usage().unwrap();
    usage.skills.get_mut("drifting").unwrap().last_activity_at = stamped;
    repo.save_usage(&usage).unwrap();

    curate(&lib, now, &CuratorConfig::default()).unwrap();
    let after = repo.load_usage().unwrap();
    assert_eq!(after.skills["drifting"].state, SkillState::Stale);
    assert!(
        (after.skills["drifting"].last_activity_at - stamped).abs() < 1.0,
        "the stale transition must not count as activity"
    );

    // 60 more days of nothing, and it reaches the archive threshold on time.
    let report = curate(&lib, now + 60.0 * 86_400.0, &CuratorConfig::default()).unwrap();
    assert_eq!(report.archived, vec!["drifting"]);
}
