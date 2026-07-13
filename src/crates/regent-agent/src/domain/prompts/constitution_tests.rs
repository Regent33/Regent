//! Unit tests for `constitution` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn constitution_ships_with_all_sixteen_sections() {
    for n in 1..=16 {
        assert!(
            CONSTITUTIONAL_PROMPT.contains(&format!("## {n}. ")),
            "section {n} missing"
        );
    }
    assert!(CONSTITUTIONAL_PROMPT.contains("## 11. Hard boundaries"));
}

#[test]
fn constitution_text_resolves_the_name_placeholder() {
    let t = constitution_text("Regent");
    assert!(t.starts_with("You are Regent."));
    assert!(!t.contains("[Agent Name]"));
}

#[test]
fn sections_parse_completely_and_in_order() {
    let sections = constitution_sections();
    assert_eq!(sections.len(), 16);
    for (i, s) in sections.iter().enumerate() {
        assert_eq!(usize::from(s.number), i + 1);
        assert!(!s.title.is_empty());
        assert!(!s.body.is_empty(), "section {} has no body", s.number);
    }
}

#[test]
fn core_keeps_safety_sections_verbatim_and_indexes_the_rest() {
    let core = constitution_core("Regent");
    assert!(core.starts_with("You are Regent."));
    assert!(core.contains("## 11. Hard boundaries"));
    assert!(core.contains("## 12. Crisis and safety response"));
    assert!(core.contains("## 14. Minors and healthy attachment"));
    assert!(
        core.contains("memory_search"),
        "must point at the memory tool"
    );
    assert!(!core.contains("## 1. Foundation"), "indexed, not inlined");
    assert!(
        core.len() < constitution_text("Regent").len() * 3 / 4,
        "core must be meaningfully smaller than the full document"
    );
}

#[test]
fn chunks_fit_the_memory_cap_with_unique_names() {
    let chunks = constitution_chunks();
    assert!(chunks.len() >= 16, "at least one chunk per section");
    let mut names = std::collections::HashSet::new();
    for (name, content) in &chunks {
        assert!(names.insert(name.clone()), "duplicate node name {name}");
        assert!(
            content.chars().count() <= 2_000,
            "{name} exceeds the entry cap"
        );
        assert!(
            content.starts_with("[Constitution §"),
            "{name} lacks its prefix"
        );
    }
}
