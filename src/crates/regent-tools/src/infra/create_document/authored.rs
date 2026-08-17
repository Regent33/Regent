//! The model-authored HTML escape hatch for PDFs.
//!
//! ADR-040 shipped themed rendering but named the remaining gap itself: "the
//! quality gap is the design layer". The template varies colour and font per
//! document; it never varies LAYOUT. Every PDF is a cover plus
//! accent-underlined sections with dot bullets, which is right for a report and
//! wrong for a pitch deck, an invoice, or a résumé — documents whose shape is
//! the whole point.
//!
//! So the model may hand over a complete HTML document and own the design,
//! with the built-in template as the fallback when it doesn't.

/// Ceiling on model-authored markup. Generous enough for a real multi-page
/// document with inline CSS, small enough that a runaway generation can't hand
/// Chromium something absurd.
pub(super) const MAX_AUTHORED_HTML: usize = 400_000;

/// Whether `html` is usable as a document, and why not when it isn't.
///
/// Deliberately permissive about STRUCTURE — no doctype or `<html>` wrapper is
/// required, because Chromium renders a bare fragment fine and demanding
/// boilerplate would just make the model emit it. The checks that matter are
/// "is there anything here" and "is it bounded".
pub(super) fn usable_html(html: &str) -> Result<&str, String> {
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return Err("authored html is empty".to_owned());
    }
    if trimmed.len() > MAX_AUTHORED_HTML {
        return Err(format!(
            "authored html is {} bytes — over the {MAX_AUTHORED_HTML}-byte limit",
            trimmed.len()
        ));
    }
    Ok(trimmed)
}

/// Wrap a bare fragment so print CSS has something to hang off. Markup that
/// already declares its own document is passed through untouched — the model
/// setting `@page` margins or a font import must win over anything added here.
pub(super) fn as_document(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<html") {
        return html.to_owned();
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <style>@page{{size:A4;margin:18mm 16mm}}\
         body{{margin:0;font-family:system-ui,-apple-system,\"Segoe UI\",sans-serif;\
         line-height:1.5}}</style></head><body>{html}</body></html>"
    )
}

#[cfg(test)]
#[path = "tests/authored.rs"]
mod tests;
