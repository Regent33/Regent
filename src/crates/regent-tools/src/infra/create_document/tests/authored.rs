//! Unit tests for the model-authored HTML escape hatch.

use super::*;

#[test]
fn real_markup_is_accepted_and_trimmed() {
    let html = usable_html("  <h1>Invoice</h1>  ").unwrap();
    assert_eq!(html, "<h1>Invoice</h1>");
}

#[test]
fn empty_or_blank_markup_is_refused() {
    assert!(usable_html("").is_err());
    assert!(usable_html("   \n\t ").is_err());
}

#[test]
fn oversized_markup_is_refused_rather_than_handed_to_the_browser() {
    let huge = "x".repeat(MAX_AUTHORED_HTML + 1);
    let err = usable_html(&huge).unwrap_err();
    assert!(err.contains("over the"), "the limit is stated: {err}");
}

/// A bare fragment gets a document wrapper so print CSS applies.
#[test]
fn a_fragment_is_wrapped_with_page_setup() {
    let doc = as_document("<h1>Hi</h1>");
    assert!(doc.starts_with("<!doctype html>"));
    assert!(doc.contains("@page"));
    assert!(doc.contains("<h1>Hi</h1>"));
}

/// A full document passes through untouched — the model's own @page rules,
/// font imports, and head content must not be overridden by ours.
#[test]
fn a_full_document_is_left_alone() {
    let authored = "<!DOCTYPE html><html><head><style>@page{size:A5}</style></head>\
                    <body><p>mine</p></body></html>";
    assert_eq!(as_document(authored), authored);
}

#[test]
fn an_html_tag_without_a_doctype_still_counts_as_a_document() {
    let authored = "<html><body><p>mine</p></body></html>";
    assert_eq!(as_document(authored), authored);
}
