//! W5, the two owner decisions of 2026-07-28: adopting untracked skills,
//! and the minimum-exposure window. Split from `curator_starvation.rs`
//! (file-size rule).

use crate::library;
use regent_skills::{
    CurationAction, CuratorConfig, FsSkillRepository, SkillRepository, curate, plan_curation,
};

/// Owner decision 1: `curate` no longer ignores skills with no telemetry row —
/// but it **adopts** them rather than archiving them.
///
/// The alternative, treating a missing row as infinite idleness, was measured
/// against the live library and would have retired skills created THIS WEEK:
/// all 13 have file mtimes of 1–9 days. "Unknown" is not "idle".
#[test]
fn an_untracked_skill_is_adopted_with_a_fresh_clock_not_archived() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create("orphan", "Agent skill with no telemetry.", "body", "agent")
        .unwrap();
    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let mut usage = repo.load_usage().unwrap();
    usage.skills.remove("orphan");
    repo.save_usage(&usage).unwrap();

    let now = 1_000_000_000.0;
    let report = curate(&lib, now, &CuratorConfig::default()).unwrap();
    assert!(report.archived.is_empty(), "adoption is not a retirement");

    let adopted = &repo.load_usage().unwrap().skills["orphan"];
    assert_eq!(
        adopted.last_activity_at, now,
        "the clock starts when the curator first sees it"
    );
    assert!(repo.list_names().unwrap().contains(&"orphan".to_owned()));

    // And from there the ordinary rules apply: idle past the threshold, with
    // exposure accrued, it retires like anything else.
    let later = now + 200.0 * 86_400.0;
    let mut usage = repo.load_usage().unwrap();
    usage.skills.get_mut("orphan").unwrap().visible_since = Some(now);
    repo.save_usage(&usage).unwrap();
    let report = curate(&lib, later, &CuratorConfig::default()).unwrap();
    assert_eq!(report.archived, vec!["orphan"]);
}

/// Owner decision 2: the minimum-exposure guarantee. Visible *now* says nothing
/// about opportunity — a skill hidden for 89 of its 90 idle days and promoted
/// yesterday has been reachable for a day. It must wait out the window.
#[test]
fn a_recently_promoted_skill_waits_for_its_exposure_window() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create("just-surfaced", "Agent skill long buried.", "body", "agent")
        .unwrap();
    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let now = 1_000_000_000.0;
    let mut usage = repo.load_usage().unwrap();
    let record = usage.skills.get_mut("just-surfaced").unwrap();
    record.last_activity_at = now - 200.0 * 86_400.0;
    record.visible_since = Some(now - 1.0 * 86_400.0); // reachable for one day
    repo.save_usage(&usage).unwrap();

    let plan = plan_curation(&lib, now, &CuratorConfig::default()).unwrap();
    assert_eq!(plan[0].action, CurationAction::AwaitingExposure);
    assert!(
        curate(&lib, now, &CuratorConfig::default())
            .unwrap()
            .archived
            .is_empty()
    );

    // A week later, having stayed reachable throughout, it retires.
    let later = now + 8.0 * 86_400.0;
    assert_eq!(
        curate(&lib, later, &CuratorConfig::default())
            .unwrap()
            .archived,
        vec!["just-surfaced"]
    );
}

/// The window is one of *continuous* reachability. Dropping out of the index
/// restarts it — otherwise a skill that flickered in and out for a year would
/// accumulate credit it never actually had.
#[test]
fn falling_out_of_the_index_restarts_the_exposure_window() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    for i in 0..30 {
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
    for i in 0..30 {
        usage
            .skills
            .get_mut(&format!("skill-{i:02}"))
            .unwrap()
            .last_activity_at = now - (i as f64) * 86_400.0;
    }
    repo.save_usage(&usage).unwrap();

    curate(&lib, now, &CuratorConfig::default()).unwrap();
    let usage = repo.load_usage().unwrap();
    let visible = lib.index_visible_names().unwrap();
    for (name, record) in &usage.skills {
        assert_eq!(
            record.visible_since.is_some(),
            visible.contains(name),
            "{name}: exposure clock must track visibility exactly"
        );
    }
}
