//! Unit tests for `mcp_tools` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;
use serde_json::json;

#[test]
fn repairs_quoted_and_schemeless_urls() {
    // The exact shape from the field report: leading quote + dropped colon.
    let out =
        sanitize_url_args(json!({"url": "\"https//search.brave.com/search?q=Claude+fable+5\""}));
    assert_eq!(
        out["url"],
        "https://search.brave.com/search?q=Claude+fable+5"
    );
}

#[test]
fn leaves_well_formed_urls_and_other_args_untouched() {
    let out = sanitize_url_args(json!({"url": "https://example.com/x", "selector": "\"#id\""}));
    assert_eq!(out["url"], "https://example.com/x");
    assert_eq!(out["selector"], "\"#id\""); // only `url` is touched
}
