//! Archive/unarchive lifecycle: opt-out, restore, and archived visibility.

use crate::library;
use regent_skills::{FsSkillRepository, SkillError, SkillRepository, SkillState};

#[test]
fn archive_then_unarchive_restores_the_skill() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    lib.create("triage", "Triage incoming issues.", "body", "user")
        .unwrap();

    // opt-out: archived skills drop out of the active list (bundled remain).
    lib.archive("triage").unwrap();
    assert!(!lib.list().unwrap().iter().any(|s| s.name == "triage"));
    assert!(dir.path().join(".archive/triage/SKILL.md").exists());

    // opt-in: restored to the active set, telemetry back to Active.
    lib.unarchive("triage").unwrap();
    let summaries = lib.list().unwrap();
    assert!(summaries.iter().any(|s| s.name == "triage"));
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
    assert!(
        !lib.list().unwrap().iter().any(|s| s.name == "retired"),
        "gone from active list"
    );

    let record = lib.view("retired").expect("archived skill views by name");
    assert_eq!(record.meta.name, "retired");
    assert_eq!(record.body, "the body");

    // A name that exists nowhere is still an honest miss.
    assert!(matches!(lib.view("ghost"), Err(SkillError::NotFound(_))));
}
