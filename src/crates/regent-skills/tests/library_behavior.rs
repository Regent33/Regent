//! Skills library + curator behavior contract against the real filesystem
//! repository.

use regent_skills::{
    CuratorConfig, FsSkillRepository, SkillError, SkillLibrary, SkillRepository, SkillState, curate,
};
use std::sync::Arc;

fn library(dir: &std::path::Path) -> SkillLibrary {
    SkillLibrary::new(Arc::new(FsSkillRepository::new(dir).unwrap()))
}

#[test]
fn create_list_view_round_trip_with_progressive_disclosure() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create(
        "code-review",
        "Structured code review workflow.",
        "# Steps\n1. read diff",
        "agent",
    )
    .unwrap();

    let summaries = lib.list().unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "code-review");

    let record = lib.view("code-review").unwrap();
    assert!(record.body.contains("read diff"));
    assert_eq!(record.meta.created_by, "agent");

    // level 2: reference file, with containment enforced
    std::fs::create_dir_all(dir.path().join("code-review/references")).unwrap();
    std::fs::write(
        dir.path().join("code-review/references/r.md"),
        "ref content",
    )
    .unwrap();
    assert_eq!(
        lib.view_file("code-review", "references/r.md").unwrap(),
        "ref content"
    );
    assert!(matches!(
        lib.view_file("code-review", "../../Cargo.toml"),
        Err(SkillError::PathEscape(_) | SkillError::NotFound(_))
    ));

    // index renders for the stable prompt tier
    let index = lib.render_index().unwrap();
    assert!(index.contains("- code-review: Structured code review workflow."));
}

#[test]
fn hardline_standards_enforced_on_create() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    // description >60 chars rejected
    assert!(
        lib.create("x", &format!("{}.", "d".repeat(70)), "b", "agent")
            .is_err()
    );
    // description must end with a period
    assert!(lib.create("x", "No period", "b", "agent").is_err());
    // bad names rejected
    assert!(
        lib.create("../escape", "Fine description.", "b", "agent")
            .is_err()
    );
    assert!(
        lib.create("Has Spaces", "Fine description.", "b", "agent")
            .is_err()
    );
    // duplicates rejected
    lib.create("ok-skill", "Fine description.", "b", "agent")
        .unwrap();
    assert!(matches!(
        lib.create("ok-skill", "Fine description.", "b", "agent"),
        Err(SkillError::AlreadyExists(_))
    ));
}

#[test]
fn archive_then_unarchive_restores_the_skill() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create("triage", "Triage incoming issues.", "body", "user")
        .unwrap();

    // opt-out: archived skills drop out of the active list.
    lib.archive("triage").unwrap();
    assert!(lib.list().unwrap().is_empty());
    assert!(dir.path().join(".archive/triage/SKILL.md").exists());

    // opt-in: restored to the active set, telemetry back to Active.
    lib.unarchive("triage").unwrap();
    let summaries = lib.list().unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "triage");
    assert!(!dir.path().join(".archive/triage").exists());
    let repo = FsSkillRepository::new(dir.path()).unwrap();
    assert_eq!(
        repo.load_usage().unwrap().skills["triage"].state,
        SkillState::Active
    );

    // Unarchiving a name that isn't archived is a clear error.
    assert!(matches!(
        lib.unarchive("triage"),
        Err(SkillError::AlreadyExists(_))
    ));
    assert!(matches!(
        lib.unarchive("ghost"),
        Err(SkillError::NotFound(_))
    ));
}

#[test]
fn archived_skill_is_still_viewable_by_name() {
    // The Skills UI lists archived rows; clicking one must show its body, not
    // "skill not found" (repo.load falls back to .archive).
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create("retired", "A retired skill.", "the body", "user")
        .unwrap();
    lib.archive("retired").unwrap();
    assert!(lib.list().unwrap().is_empty(), "gone from active list");

    let record = lib.view("retired").expect("archived skill views by name");
    assert_eq!(record.meta.name, "retired");
    assert_eq!(record.body, "the body");

    // A name that exists nowhere is still an honest miss.
    assert!(matches!(lib.view("ghost"), Err(SkillError::NotFound(_))));
}

#[test]
fn patch_requires_exactly_one_occurrence_and_bumps_telemetry() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create(
        "deploy",
        "Deploy the service safely.",
        "step A\nstep B\nstep A",
        "agent",
    )
    .unwrap();

    assert!(matches!(
        lib.patch("deploy", "step A", "step Z"),
        Err(SkillError::PatchMismatch(_))
    ));
    lib.patch("deploy", "step B", "step B with checks").unwrap();
    assert!(
        lib.view("deploy")
            .unwrap()
            .body
            .contains("step B with checks")
    );

    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let usage = repo.load_usage().unwrap();
    assert_eq!(usage.skills["deploy"].patch_count, 1);
    assert!(usage.skills["deploy"].view_count >= 1);
}

#[test]
fn curator_archives_stale_agent_skills_but_never_pinned_or_user_ones() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create("old-agent-skill", "Old agent skill.", "body", "agent")
        .unwrap();
    lib.create("fresh-agent-skill", "Fresh agent skill.", "body", "agent")
        .unwrap();
    lib.create("old-user-skill", "Old user skill.", "body", "user")
        .unwrap();
    lib.create("old-pinned-skill", "Old pinned skill.", "body", "agent")
        .unwrap();

    // Backdate telemetry + pin via direct repo access (fixture setup).
    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let now = 1_000_000_000.0;
    let mut usage = repo.load_usage().unwrap();
    for name in ["old-agent-skill", "old-user-skill", "old-pinned-skill"] {
        usage.skills.get_mut(name).unwrap().last_activity_at = now - 100.0 * 86_400.0;
    }
    usage
        .skills
        .get_mut("fresh-agent-skill")
        .unwrap()
        .last_activity_at = now - 40.0 * 86_400.0;
    repo.save_usage(&usage).unwrap();
    let pinned = repo.load("old-pinned-skill").unwrap();
    let mut pinned_meta = pinned.meta.clone();
    pinned_meta.pinned = true;
    repo.save(&pinned_meta, &pinned.body).unwrap();

    let report = curate(&lib, now, &CuratorConfig::default()).unwrap();

    // 100 days idle agent skill → archived; 40 days idle → stale; user +
    // pinned untouched.
    assert_eq!(report.archived, vec!["old-agent-skill"]);
    assert_eq!(report.marked_stale, vec!["fresh-agent-skill"]);
    let names = repo.list_names().unwrap();
    assert!(!names.contains(&"old-agent-skill".to_owned()));
    assert!(names.contains(&"old-user-skill".to_owned()));
    assert!(names.contains(&"old-pinned-skill".to_owned()));
    // never deleted — it lives in .archive/
    assert!(
        dir.path()
            .join(".archive/old-agent-skill/SKILL.md")
            .exists()
    );
    assert_eq!(
        repo.load_usage().unwrap().skills["fresh-agent-skill"].state,
        SkillState::Stale
    );

    // explicit archive of a pinned skill also refuses
    assert!(matches!(
        lib.archive("old-pinned-skill"),
        Err(SkillError::Pinned(_))
    ));
}

// SPL P4 (§3.4): past 24 skills the index renders only the most-recently-used
// lines plus a "…and K more" pointer; at or under the threshold it's complete.
#[test]
fn index_caps_at_mru_24_past_the_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    for i in 0..30 {
        lib.create(
            &format!("skill-{i:02}"),
            "Some fine description here.",
            "b",
            "agent",
        )
        .unwrap();
    }
    // A view stamps last_activity_at — this one must survive the cap even
    // though creation order would place it last alphabetically.
    lib.view("skill-29").unwrap();

    let index = lib.render_index().unwrap();
    let lines = index.matches("\n- ").count();
    assert_eq!(lines, 25, "24 skill lines + the overflow pointer: {index}");
    assert!(index.contains("- skill-29:"), "recently-used survives");
    assert!(index.contains("…and 6 more — skills_list shows all."));

    // At the threshold, no cap and no pointer.
    let small = tempfile::tempdir().unwrap();
    let lib2 = library(small.path());
    for i in 0..3 {
        lib2.create(
            &format!("s-{i}"),
            "Some fine description here.",
            "b",
            "agent",
        )
        .unwrap();
    }
    let idx = lib2.render_index().unwrap();
    assert!(!idx.contains("more — skills_list"));
    assert_eq!(idx.matches("\n- ").count(), 3);
}
