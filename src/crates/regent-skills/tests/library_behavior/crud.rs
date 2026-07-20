//! Create/list/view/patch round-trips and the hardline creation standards.

use crate::library;
use regent_skills::{FsSkillRepository, SkillError, SkillRepository};

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

    // 1 disk skill + the 14 bundled ones (ponytail, code-reviewer,
    // secure-code-guardian, documents, research, plus the 8 Hermes ports)
    // that ship in the binary.
    let summaries = lib.list().unwrap();
    assert_eq!(summaries.len(), 15);
    // "arxiv" (bundled) now sorts ahead of it — the disk skill must be listed.
    assert!(summaries.iter().any(|s| s.name == "code-review"));

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
