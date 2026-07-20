//! Palette + font pairings. One resolved `Theme` drives both the HTML/CSS report
//! (PDF) and the PptxGenJS deck, so a document reads as one designed system.
//!
//! The catalog is NOT the ceiling — it is a set of named starting points. The
//! model can instead pass a full custom palette (any colors, any fonts) per
//! document. When the model names nothing — which, in practice, is most of the
//! time — the palette is GENERATED from the content seed (`palette::generate`)
//! rather than drawn from the catalog: five presets meant one document in five
//! looked like another, which is the "everything looks the same" complaint.
//! Variety is then multiplicative: generated theme × per-slide layout × content.

use super::palette;
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

/// Resolve a concrete theme: a named preset, a custom palette (overlaid on a
/// base or on the generated default), or — when nothing is asked — a palette
/// generated from the content.
#[must_use]
pub fn resolve(choice: Option<&ThemeChoice>, seed: &str) -> Theme {
    match choice {
        Some(ThemeChoice::Named(name)) => {
            preset_by_name(name).map_or_else(|| palette::generate(seed), Preset::to_theme)
        }
        Some(ThemeChoice::Custom(custom)) => {
            let base = custom
                .base
                .as_deref()
                .and_then(preset_by_name)
                .map_or_else(|| palette::generate(seed), Preset::to_theme);
            overlay(base, custom)
        }
        None => palette::generate(seed),
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

    /// No theme asked for → a generated palette, not one of five presets. Eight
    /// documents must produce eight looks; the old catalog lottery could only
    /// manage five, and collided long before this.
    #[test]
    fn the_default_is_generated_and_differs_per_document() {
        let accents: std::collections::HashSet<_> = [
            "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel",
        ]
        .iter()
        .map(|seed| resolve(None, seed).accent)
        .collect();
        assert_eq!(accents.len(), 8, "default palettes collided: {accents:?}");
    }
}
