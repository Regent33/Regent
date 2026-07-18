//! Build the PptxGenJS deck spec (the JSON the `__render` sidecar consumes) from
//! a `DocumentSpec`. Each slide's layout comes from its hint or is chosen from
//! its content, so a deck varies slide to slide instead of one fixed recipe.

use super::model::{DocumentSpec, Slide};
use super::theme::Theme;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

const VALID_LAYOUTS: &[&str] = &[
    "cover", "content", "section", "split", "chart", "grid", "blank",
];

/// The `{theme, slides}` object the renderer's `pptx` job expects.
pub fn build_spec(spec: &DocumentSpec, theme: &Theme) -> Value {
    let total = spec.slides.len();
    let slides: Vec<Value> = spec
        .slides
        .iter()
        .enumerate()
        .map(|(index, slide)| slide_json(slide, index, total))
        .collect();
    json!({ "theme": theme, "slides": slides })
}

fn slide_json(slide: &Slide, index: usize, total: usize) -> Value {
    let layout = slide
        .layout
        .as_deref()
        .map(|hint| hint.trim().to_ascii_lowercase())
        .filter(|hint| VALID_LAYOUTS.contains(&hint.as_str()))
        .unwrap_or_else(|| auto_layout(slide, index, total));

    let mut out = json!({ "layout": layout, "title": slide.title });
    if let Some(subtitle) = &slide.subtitle {
        out["subtitle"] = json!(subtitle);
    }
    if !slide.bullets.is_empty() {
        out["bullets"] = json!(slide.bullets);
    }
    if let Some(notes) = &slide.notes {
        out["notes"] = json!(notes);
    }
    if let Some(image) = &slide.embedded_image {
        // The hydrator already normalized to PNG bytes; the deck carries them
        // base64 (no data: prefix — the TS side adds it).
        out["imageBase64"] = json!(BASE64.encode(&image.bytes));
    }
    out
}

/// Pick a layout from the slide's shape when the model gave no hint: slide 1 is
/// the cover, an imaged slide splits, a bare titled slide in a longer deck acts
/// as a section divider, everything else is content.
fn auto_layout(slide: &Slide, index: usize, total: usize) -> String {
    if index == 0 {
        return "cover".to_owned();
    }
    if slide.embedded_image.is_some() {
        return "split".to_owned();
    }
    if total > 4 && slide.bullets.is_empty() && slide.subtitle.is_some() {
        return "section".to_owned();
    }
    "content".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::create_document::theme;
    use serde_json::from_value;

    #[test]
    fn deck_spec_uses_camelcase_theme_and_varied_layouts() {
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [
                {"title": "Cover"},
                {"title": "Body", "bullets": ["a", "b"]},
                {"title": "Forced", "layout": "chart"}
            ]
        }))
        .unwrap();
        let resolved = theme::resolve(None, "seed");
        let deck = build_spec(&spec, &resolved);

        // camelCase keys the TS DeckTheme requires.
        assert!(deck["theme"]["coverBackground"].is_string());
        assert!(deck["theme"]["titleFont"].is_string());
        // Auto: first is cover, second is content; explicit hint wins on third.
        assert_eq!(deck["slides"][0]["layout"], "cover");
        assert_eq!(deck["slides"][1]["layout"], "content");
        assert_eq!(deck["slides"][2]["layout"], "chart");
    }

    #[test]
    fn invalid_layout_hint_falls_back_to_auto() {
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "A"}, {"title": "B", "layout": "hexagon"}]
        }))
        .unwrap();
        let deck = build_spec(&spec, &theme::resolve(None, "s"));
        assert_eq!(deck["slides"][1]["layout"], "content");
    }
}
