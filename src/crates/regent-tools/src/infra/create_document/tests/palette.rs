//! The generated-palette contract: many distinct looks, legible in every hue,
//! and stable for a given document.

use super::*;
use std::collections::HashSet;

/// The whole point of generating instead of picking from a catalog of five.
#[test]
fn many_documents_get_many_distinct_looks() {
    let looks: HashSet<_> = (0..60)
        .map(|i| {
            let theme = generate(&format!("Document number {i}"));
            (theme.accent, theme.cover_background, theme.title_font)
        })
        .collect();
    assert!(
        looks.len() > 40,
        "generated palettes collapsed to {} looks out of 60",
        looks.len()
    );
}

#[test]
fn the_same_document_always_regenerates_the_same_look() {
    // An `operation: "edit"` re-render must not reshuffle the design.
    assert_eq!(generate("Q3 Review").accent, generate("Q3 Review").accent);
}

/// Every hue must produce an accent dark enough to carry the cover text that the
/// `section` slide layout puts on it — the luminance clamp, not lightness,
/// guarantees this (yellow and blue at equal HSL lightness are nowhere near
/// equally bright).
#[test]
fn every_hue_yields_a_legible_accent() {
    for hue in (0..360).step_by(5) {
        let hue = f64::from(hue);
        let accent = unhex(&accent_hex(hue, 0.7));
        // The real on-screen pairing, not white: the same cover_text `generate`
        // would produce for this hue.
        let cover_text = unhex(&hsl_hex(hue, 0.7 * 0.22, 0.97));
        let ratio = (luminance(cover_text) + 0.05) / (luminance(accent) + 0.05);
        assert!(ratio >= 4.5, "hue {hue}: contrast {ratio:.2} below WCAG AA");
    }
}

#[test]
fn body_text_stays_readable_on_the_generated_paper() {
    for seed in ["Alpha", "Bravo", "Charlie", "Delta", "Echo"] {
        let theme = generate(seed);
        let (text, paper) = (unhex(&theme.text), unhex(&theme.background));
        let ratio = (luminance(paper) + 0.05) / (luminance(text) + 0.05);
        assert!(ratio >= 7.0, "{seed}: body contrast {ratio:.2} too low");
    }
}

fn unhex(value: &str) -> (f64, f64, f64) {
    let channel = |i: usize| {
        f64::from(u8::from_str_radix(&value[i..i + 2], 16).expect("6-hex color")) / 255.0
    };
    (channel(0), channel(2), channel(4))
}
