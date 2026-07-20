//! Bundled skills: provenance, disk overrides, and the MRU-capped index.

use crate::library;

#[test]
fn bundled_skills_list_view_and_yield_to_disk_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());

    // Present out of the box, with bundled provenance.
    let names: Vec<String> = lib.list().unwrap().into_iter().map(|s| s.name).collect();
    for name in ["ponytail", "code-reviewer", "secure-code-guardian"] {
        assert!(
            names.contains(&name.to_owned()),
            "{name} missing: {names:?}"
        );
    }
    let record = lib.view("ponytail").unwrap();
    assert_eq!(record.meta.created_by, "bundled");
    assert!(record.body.contains("ladder"), "the YAGNI ladder ships");

    // A disk skill with the same name overrides the bundled one entirely.
    lib.create("ponytail", "My own ponytail variant.", "my body", "user")
        .unwrap();
    let record = lib.view("ponytail").unwrap();
    assert_eq!(record.body, "my body");
    let summaries = lib.list().unwrap();
    let mine: Vec<_> = summaries.iter().filter(|s| s.name == "ponytail").collect();
    assert_eq!(mine.len(), 1, "no duplicate listing");
    assert_eq!(mine[0].description, "My own ponytail variant.");

    // A bundled-only skill has no disk directory to archive.
    assert!(lib.archive("code-reviewer").is_err());
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

    // 30 disk + 14 bundled = 44 total → 24 lines + the overflow pointer.
    let index = lib.render_index().unwrap();
    let lines = index.matches("\n- ").count();
    assert_eq!(lines, 25, "24 skill lines + the overflow pointer: {index}");
    assert!(index.contains("- skill-29:"), "recently-used survives");
    assert!(index.contains("…and 20 more — skills_list shows all."));

    // Under the threshold (3 disk + 14 bundled), no cap and no pointer.
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
    assert_eq!(idx.matches("\n- ").count(), 17);
}
