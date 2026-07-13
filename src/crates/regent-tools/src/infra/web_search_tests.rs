//! Unit tests for `web_search` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn strips_html_to_text() {
    let html = "<html><head><style>x{}</style></head><body><p>Hello &amp; \
                <b>world</b></p><script>bad()</script></body></html>";
    assert_eq!(html_to_text(html), "Hello & world");
}
