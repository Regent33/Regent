//! Curator policy: stale agent skills archive; pinned, user, and bundled
//! provenance are never touched.

use crate::library;
use regent_skills::{
    CuratorConfig, FsSkillRepository, SkillError, SkillRepository, SkillState, curate,
};

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

#[test]
fn curator_never_touches_bundled_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    // Even a DISK skill marked bundled (the override path) is out of curator
    // scope — the guard is on created_by, not on where the file lives.
    lib.create(
        "impostor",
        "Disk skill with bundled tag.",
        "body",
        "bundled",
    )
    .unwrap();
    let repo = FsSkillRepository::new(dir.path()).unwrap();
    let now = 1_000_000_000.0;
    let mut usage = repo.load_usage().unwrap();
    usage.skills.get_mut("impostor").unwrap().last_activity_at = now - 400.0 * 86_400.0;
    repo.save_usage(&usage).unwrap();

    let report = curate(&lib, now, &CuratorConfig::default()).unwrap();
    assert!(report.archived.is_empty());
    assert!(report.marked_stale.is_empty());
    assert!(repo.list_names().unwrap().contains(&"impostor".to_owned()));
}
