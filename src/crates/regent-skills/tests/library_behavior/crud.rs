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

/// The index hook is capped in BYTES, measured on the rendered output.
///
/// Round 8, HIGH-1. This index is one of the three stores the deacon's Tier-1
/// ceiling is the sum of, and that ceiling measures bytes. While the cap counted
/// CHARS, a 140-character CJK description rendered 420 bytes where the ceiling
/// had reserved 140 — 24 such skills overshot by 6,720 against 1,951 bytes of
/// headroom, and `cap_tier1` cleared the memory block to make room.
///
/// Asserted against the REAL rendered string rather than a product of constants,
/// because a constant cannot see its own unit — which is exactly how this
/// survived two commits that claimed to have converted it.
#[test]
fn a_non_latin_index_line_is_capped_in_bytes_on_the_rendered_output() {
    let dir = tempfile::tempdir().unwrap();
    let lib = library(dir.path());
    // Hand-authored SKILL.md is the documented way to add a skill, and the READ
    // path never length-validates the description — so this reaches the index
    // unchecked, which is precisely why the hook cap has to hold here.
    let skill = dir.path().join("long-cjk");
    std::fs::create_dir_all(&skill).unwrap();
    // 200 CJK characters = 600 bytes, well past the 140-byte hook cap.
    let description = "経".repeat(200);
    std::fs::write(
        skill.join("SKILL.md"),
        format!("---\nname: long-cjk\ndescription: {description}\n---\n# body\n"),
    )
    .unwrap();

    let index = lib.render_index().unwrap();
    let line = index
        .lines()
        .find(|l| l.starts_with("- long-cjk:"))
        .expect("the hand-authored skill renders");
    let hook = line.trim_start_matches("- long-cjk: ");
    assert!(
        hook.len() <= regent_skills::SKILLS_INDEX_HOOK_BYTES,
        "hook is {} bytes, over the {}-byte cap — a char cap would read {} here \
         and let the index overrun its share of the Tier-1 ceiling",
        hook.len(),
        regent_skills::SKILLS_INDEX_HOOK_BYTES,
        hook.chars().count()
    );
    assert!(hook.ends_with('…'), "truncation is marked: {hook}");
    // Every line, not just this one, so the bound is about the index and not
    // about one fixture.
    for l in index.lines().filter(|l| l.starts_with("- ")) {
        assert!(
            l.len() <= regent_skills::SKILLS_INDEX_HOOK_BYTES + 80,
            "index line over its budget ({} bytes): {l}",
            l.len()
        );
    }
}
