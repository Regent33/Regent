//! Unit tests for `profile_ops` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::profile_size_json;

#[test]
fn profile_size_json_reports_chars_and_chars_over_four_tokens() {
    let prompt = "a".repeat(400);
    let defs = "b".repeat(100);
    let v = profile_size_json(&(prompt, defs));
    assert_eq!(v["prompt_chars"], 400);
    assert_eq!(v["prompt_est_tokens"], 100);
    assert_eq!(v["defs_chars"], 100);
    assert_eq!(v["defs_est_tokens"], 25);
    assert_eq!(v["total_est_tokens"], 125);
}
