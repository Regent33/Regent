//! Palette + font pairings. One resolved `Theme` drives both the HTML/CSS report
//! (PDF) and the PptxGenJS deck, so a document reads as one designed system.
//!
//! The catalog is NOT the ceiling — it is a set of good defaults. The model can
//! instead pass a full custom palette (any colors, any fonts) per document, so
//! the design space is open. When the model names nothing, the default is chosen
//! from the content (`seed`), so two different documents don't come out
//! identical — the whole point of this redesign. Variety is then multiplicative:
//! theme × per-slide layout × content.

use serde::{Deserialize, Serialize};

/// A fully-resolved look. Owned, because it may come from the catalog OR from a
/// theme the model designed. Colors are 6-hex, no leading '#'. Fonts are common
/// system faces so a .pptx opens correctly without embedding (PptxGenJS cannot
/// embed fonts); the HTML path wraps them in a fuller stack.
///
/// Serializes camelCase to match the TS renderer's `DeckTheme` (coverBackground,
/// titleFont, …); the HTML template reads the same camelCase keys.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub background: String,
    pub text: String,
    pub accent: String,
    pub muted: String,
    pub cover_background: String,
    pub cover_text: String,
    pub title_font: String,
    pub body_font: String,
}

/// What the model may pass as `theme`: a preset name (JSON string) or a custom
/// palette (JSON object). Untagged, so both deserialize from the one field.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ThemeChoice {
    Named(String),
    Custom(CustomTheme),
}

/// A model-designed theme. Every field optional; anything omitted inherits from
/// `base` (a preset name) or, failing that, the seeded default. This is what
/// keeps the palette space open rather than limited to the catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomTheme {
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub muted: Option<String>,
    #[serde(default)]
    pub cover_background: Option<String>,
    #[serde(default)]
    pub cover_text: Option<String>,
    #[serde(default)]
    pub title_font: Option<String>,
    #[serde(default)]
    pub body_font: Option<String>,
}

struct Preset {
    name: &'static str,
    background: &'static str,
    text: &'static str,
    accent: &'static str,
    muted: &'static str,
    cover_background: &'static str,
    cover_text: &'static str,
    title_font: &'static str,
    body_font: &'static str,
}

impl Preset {
    fn to_theme(&self) -> Theme {
        Theme {
            background: self.background.to_owned(),
            text: self.text.to_owned(),
            accent: self.accent.to_owned(),
            muted: self.muted.to_owned(),
            cover_background: self.cover_background.to_owned(),
            cover_text: self.cover_text.to_owned(),
            title_font: self.title_font.to_owned(),
            body_font: self.body_font.to_owned(),
        }
    }
}

/// Curated starting points — each a distinct mood, not a shade of the same one.
/// Callers may ignore these entirely and supply a custom palette.
const CATALOG: &[Preset] = &[
    Preset {
        name: "midnight",
        background: "FFFFFF",
        text: "171C2C",
        accent: "00A19B",
        muted: "667085",
        cover_background: "171C2C",
        cover_text: "FFFFFF",
        title_font: "Georgia",
        body_font: "Calibri",
    },
    Preset {
        name: "warm-editorial",
        background: "FBF7F0",
        text: "2B211A",
        accent: "B4472E",
        muted: "8A7A6B",
        cover_background: "2B211A",
        cover_text: "FBF7F0",
        title_font: "Cambria",
        body_font: "Verdana",
    },
    Preset {
        name: "mono",
        background: "FFFFFF",
        text: "111111",
        accent: "111111",
        muted: "9A9A9A",
        cover_background: "111111",
        cover_text: "FFFFFF",
        title_font: "Arial",
        body_font: "Arial",
    },
    Preset {
        name: "forest",
        background: "F5F8F3",
        text: "16261B",
        accent: "2E7D4F",
        muted: "5F7566",
        cover_background: "16261B",
        cover_text: "F5F8F3",
        title_font: "Cambria",
        body_font: "Calibri",
    },
    Preset {
        name: "royal",
        background: "FBFAFF",
        text: "1E1B3A",
        accent: "6D4AC4",
        muted: "6E6A88",
        cover_background: "1E1B3A",
        cover_text: "F5F2FF",
        title_font: "Georgia",
        body_font: "Trebuchet MS",
    },
];

fn preset_by_name(name: &str) -> Option<&'static Preset> {
    let name = name.trim();
    CATALOG
        .iter()
        .find(|preset| preset.name.eq_ignore_ascii_case(name))
}

fn seeded(seed: &str) -> &'static Preset {
    &CATALOG[(fnv1a(seed) as usize) % CATALOG.len()]
}

/// Resolve a concrete theme: a named preset, a custom palette (overlaid on a
/// base or the seeded default), or — when nothing is asked — the seeded default.
#[must_use]
pub fn resolve(choice: Option<&ThemeChoice>, seed: &str) -> Theme {
    match choice {
        Some(ThemeChoice::Named(name)) => preset_by_name(name)
            .unwrap_or_else(|| seeded(seed))
            .to_theme(),
        Some(ThemeChoice::Custom(custom)) => {
            let base = custom
                .base
                .as_deref()
                .and_then(preset_by_name)
                .unwrap_or_else(|| seeded(seed))
                .to_theme();
            overlay(base, custom)
        }
        None => seeded(seed).to_theme(),
    }
}

fn overlay(mut base: Theme, custom: &CustomTheme) -> Theme {
    let apply = |slot: &mut String, value: &Option<String>| {
        if let Some(value) = value {
            let value = value.trim();
            if !value.is_empty() {
                *slot = value.to_owned();
            }
        }
    };
    apply(&mut base.background, &custom.background);
    apply(&mut base.text, &custom.text);
    apply(&mut base.accent, &custom.accent);
    apply(&mut base.muted, &custom.muted);
    apply(&mut base.cover_background, &custom.cover_background);
    apply(&mut base.cover_text, &custom.cover_text);
    apply(&mut base.title_font, &custom.title_font);
    apply(&mut base.body_font, &custom.body_font);
    base
}

/// FNV-1a over the seed bytes — a tiny, dependency-free, deterministic hash.
/// Stable across runs (unlike the randomized DefaultHasher) so the same title
/// always maps to the same default theme.
fn fnv1a(seed: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn choice(value: serde_json::Value) -> ThemeChoice {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn named_preset_wins_case_insensitively() {
        assert_eq!(resolve(Some(&choice(json!("ROYAL"))), "x").accent, "6D4AC4");
    }

    #[test]
    fn custom_palette_is_honored_verbatim() {
        // A palette outside the catalog — proves the space is open, not fixed.
        let theme = resolve(
            Some(&choice(json!({"accent": "FF00AA", "background": "0A0A0A"}))),
            "x",
        );
        assert_eq!(theme.accent, "FF00AA");
        assert_eq!(theme.background, "0A0A0A");
    }

    #[test]
    fn custom_overlays_a_named_base() {
        let theme = resolve(
            Some(&choice(json!({"base": "forest", "accent": "123456"}))),
            "x",
        );
        assert_eq!(theme.accent, "123456"); // overridden
        assert_eq!(theme.text, "16261B"); // inherited from forest
    }

    #[test]
    fn seeded_default_varies_across_documents() {
        let names: std::collections::HashSet<_> = [
            "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel",
        ]
        .iter()
        .map(|seed| resolve(None, seed).accent)
        .collect();
        assert!(names.len() > 1, "seeded default never varied: {names:?}");
    }
}
