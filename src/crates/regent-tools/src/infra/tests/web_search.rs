//! Unit tests for `web_search` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn strips_html_to_text() {
    let html = "<html><head><style>x{}</style></head><body><p>Hello &amp; \
                <b>world</b></p><script>bad()</script></body></html>";
    assert_eq!(html_to_text(html), "Hello & world");
}

#[test]
fn snippet_is_capped_on_a_char_boundary() {
    // Multibyte input (each 'é' is 2 bytes): the cut must land on a char
    // boundary, never mid-byte, and never panic. Short input passes through.
    let capped = cap_snippet(&"é".repeat(600));
    assert_eq!(capped.chars().count(), SNIPPET_MAX_CHARS + 1, "+1 ellipsis");
    assert!(capped.ends_with('…'));
    assert!(capped.is_char_boundary(capped.len()), "valid UTF-8");
    assert_eq!(cap_snippet("hi"), "hi");
}

#[test]
fn results_json_bounds_snippets_keeping_count_order_and_urls() {
    let results: Vec<SearchResult> = (0..12)
        .map(|i| SearchResult {
            title: format!("t{i}"),
            url: format!("https://example.com/{i}"),
            snippet: "日".repeat(1000), // 3-byte chars, oversized
        })
        .collect();
    let out = results_json("brave", "q", &results);
    let value: Value = serde_json::from_str(&out).expect("valid JSON");
    let arr = value["results"].as_array().expect("results array");
    assert_eq!(arr.len(), 12, "all ≥12 results preserved");
    for (i, r) in arr.iter().enumerate() {
        assert_eq!(
            r["url"],
            format!("https://example.com/{i}"),
            "order + url intact"
        );
        let snippet = r["snippet"].as_str().unwrap();
        assert!(
            snippet.chars().count() <= SNIPPET_MAX_CHARS + 1,
            "snippet bounded"
        );
    }
}
