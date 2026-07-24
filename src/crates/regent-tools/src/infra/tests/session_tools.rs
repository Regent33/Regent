//! Unit tests for `session_tools` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::{SESSION_SEARCH_MAX, SNIPPET_MAX_CHARS, cap_snippet, clamp_session_limit};

#[test]
fn limit_is_clamped_to_a_sane_range() {
    assert_eq!(clamp_session_limit(None), 10, "default when unset");
    assert_eq!(clamp_session_limit(Some(5)), 5, "honored when in range");
    // An oversized request is bounded — recall can't become a full dump.
    assert_eq!(
        clamp_session_limit(Some(9999)),
        SESSION_SEARCH_MAX,
        "oversized clamped to max"
    );
    assert_eq!(
        clamp_session_limit(Some(20)),
        SESSION_SEARCH_MAX,
        "at the cap"
    );
    assert_eq!(clamp_session_limit(Some(0)), 1, "zero floored to 1");
}

#[test]
fn snippets_are_capped_on_a_char_boundary() {
    // 3-byte chars, oversized: the cut lands on a char boundary, never mid-byte.
    let capped = cap_snippet(&"漢".repeat(1000));
    assert_eq!(capped.chars().count(), SNIPPET_MAX_CHARS + 1, "+1 ellipsis");
    assert!(capped.ends_with('…'));
    assert!(capped.is_char_boundary(capped.len()), "valid UTF-8");
    // A short snippet is returned untouched.
    assert_eq!(cap_snippet("short"), "short");
}
