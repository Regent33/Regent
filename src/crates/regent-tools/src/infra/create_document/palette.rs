//! Procedurally generated palettes — the default look when the model names no
//! theme.
//!
//! The catalog in `theme` gives five curated presets. Five is not variety: two
//! documents in five collide, which is exactly the "everything Regent makes
//! looks the same" complaint. So the DEFAULT is generated instead of picked —
//! a hue, saturation, and font pairing derived from the content seed, giving a
//! effectively unbounded set of looks with no model cooperation required. The
//! model opting into a theme still wins; this only replaces the fallback.
//!
//! Colors are built in HSL (easy to reason about, easy to keep legible) and
//! emitted as 6-hex without '#', matching `Theme`.

use super::theme::Theme;

/// Title/body pairings restricted to faces present on stock Windows/macOS —
/// PptxGenJS cannot embed fonts, so an exotic pick would silently fall back to
/// Calibri and undo the variety.
const PAIRINGS: &[(&str, &str)] = &[
    ("Georgia", "Calibri"),
    ("Cambria", "Verdana"),
    ("Trebuchet MS", "Calibri"),
    ("Palatino Linotype", "Segoe UI"),
    ("Times New Roman", "Arial"),
    ("Georgia", "Trebuchet MS"),
    ("Cambria", "Segoe UI"),
    ("Tahoma", "Georgia"),
];

/// Accent luminance ceiling. A generated hue at fixed HSL lightness varies wildly
/// in perceived brightness (yellow at L=0.42 is far brighter than blue), and the
/// accent carries near-white text on `section` slides. Clamping by real relative
/// luminance keeps every generated hue legible instead of trusting lightness.
///
/// Derived, not guessed: WCAG AA needs 4.5:1, and the lightest thing we put on
/// the accent is `cover_text` (luminance ≈ 0.93), so the accent must sit at or
/// below (0.93 + 0.05) / 4.5 - 0.05 ≈ 0.168.
const ACCENT_MAX_LUMINANCE: f64 = 0.16;

/// How dark the accent may be driven while chasing that ceiling. Deep enough for
/// the worst hue (yellow), stopping short of an indistinguishable near-black.
const ACCENT_MIN_LIGHTNESS: f64 = 0.08;

/// Build a full palette from `seed`. Deterministic: the same document always
/// gets the same look, so an edit re-render doesn't reshuffle the design.
#[must_use]
pub fn generate(seed: &str) -> Theme {
    let hash = fnv1a(seed);
    let hue = (hash % 360) as f64;
    // 0.34..0.76 — the low end reads editorial/muted, the high end vivid, so
    // documents differ in mood and not just in hue.
    let saturation = 0.34 + f64::from((hash >> 16) as u32 % 7) * 0.07;
    let (title_font, body_font) = PAIRINGS[((hash >> 32) as usize) % PAIRINGS.len()];

    Theme {
        background: hsl_hex(hue, saturation * 0.20, 0.975),
        text: hsl_hex(hue, saturation * 0.45, 0.12),
        accent: accent_hex(hue, saturation),
        muted: hsl_hex(hue, saturation * 0.25, 0.42),
        cover_background: hsl_hex(hue, saturation * 0.62, 0.13),
        cover_text: hsl_hex(hue, saturation * 0.22, 0.97),
        title_font: title_font.to_owned(),
        body_font: body_font.to_owned(),
    }
}

/// The accent, darkened until it is dark enough to carry light text.
fn accent_hex(hue: f64, saturation: f64) -> String {
    let mut lightness = 0.44;
    let mut rgb = hsl_rgb(hue, saturation.max(0.5), lightness);
    while luminance(rgb) > ACCENT_MAX_LUMINANCE && lightness > ACCENT_MIN_LIGHTNESS {
        lightness -= 0.01;
        rgb = hsl_rgb(hue, saturation.max(0.5), lightness);
    }
    hex(rgb)
}

fn hsl_hex(hue: f64, saturation: f64, lightness: f64) -> String {
    hex(hsl_rgb(hue, saturation, lightness))
}

/// Standard HSL → sRGB. `hue` in degrees, `saturation`/`lightness` in 0..=1.
fn hsl_rgb(hue: f64, saturation: f64, lightness: f64) -> (f64, f64, f64) {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = (hue.rem_euclid(360.0)) / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u8 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let base = lightness - chroma / 2.0;
    (r + base, g + base, b + base)
}

/// WCAG relative luminance of an sRGB triple in 0..=1.
fn luminance((r, g, b): (f64, f64, f64)) -> f64 {
    0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b)
}

fn linearize(channel: f64) -> f64 {
    if channel <= 0.03928 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn hex((r, g, b): (f64, f64, f64)) -> String {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b))
}

/// FNV-1a over the seed bytes — tiny, dependency-free, deterministic. Stable
/// across runs (unlike the randomized `DefaultHasher`) so the same title always
/// maps to the same design.
fn fnv1a(seed: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
#[path = "tests/palette.rs"]
mod tests;
