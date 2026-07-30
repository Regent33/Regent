//! Build the PptxGenJS deck spec (the JSON the `__render` sidecar consumes) from
//! a `DocumentSpec`. Each slide's layout comes from its hint or is chosen from
//! its content, so a deck varies slide to slide instead of one fixed recipe.

use super::model::{DocumentSpec, PPTX_LAYOUTS, Slide};
use super::theme::Theme;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

/// The `{theme, slides}` object the renderer's `pptx` job expects.
pub fn build_spec(spec: &DocumentSpec, theme: &Theme) -> Value {
    let slides: Vec<Value> = spec
        .slides
        .iter()
        .enumerate()
        .map(|(index, slide)| slide_json(slide, index))
        .collect();
    json!({ "theme": theme, "slides": slides })
}

fn slide_json(slide: &Slide, index: usize) -> Value {
    let layout = slide
        .layout
        .as_deref()
        .map(|hint| hint.trim().to_ascii_lowercase())
        .filter(|hint| PPTX_LAYOUTS.contains(&hint.as_str()))
        .unwrap_or_else(|| auto_layout(slide, index));

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
    // Model-placed elements ride through untouched — the renderer owns their
    // contract. Bounded so a runaway generation can't hand the sidecar a
    // hundred thousand shapes.
    if !slide.elements.is_empty() {
        const MAX_ELEMENTS: usize = 60;
        out["elements"] = json!(slide.elements.iter().take(MAX_ELEMENTS).collect::<Vec<_>>());
    }
    if let Some(table) = &slide.table {
        // Padded here rather than in the renderer: a ragged row is a Rust-side
        // shape problem, and the TS side should receive a rectangle.
        out["table"] = json!({
            "headers": table.headers,
            "rows": table.padded_rows(),
            "caption": table.caption,
        });
    }
    if let Some(image) = &slide.embedded_image {
        // The hydrator already normalized to PNG bytes; the deck carries them
        // base64 (no data: prefix — the TS side adds it).
        out["imageBase64"] = json!(BASE64.encode(&image.bytes));
    }
    out
}

/// Pick a layout from the slide's shape when the model gave no hint.
///
/// This used to send every non-cover, non-image slide to `content`, so a deck
/// where the model set no layouts — the common case — was a cover followed by N
/// identical bullet slides. That is the single biggest cause of "every deck
/// looks the same". The rules below read the slide's shape more closely and
/// space dividers through the deck, so an unhinted deck still varies.
fn auto_layout(slide: &Slide, index: usize) -> String {
    if index == 0 {
        return "cover".to_owned();
    }
    if slide.embedded_image.is_some() {
        return "split".to_owned();
    }
    // A titled slide carrying no bullets is a divider, not an empty content
    // slide — regardless of whether the model bothered with a subtitle.
    if slide.bullets.is_empty() {
        return "section".to_owned();
    }
    // Short, parallel bullets are a set of points, which reads far better as
    // numbered cards than as another dash list. Long bullets are prose and stay
    // in `content`, where they have the width to breathe.
    let longest = slide.bullets.iter().map(|b| b.chars().count()).max();
    if (3..=6).contains(&slide.bullets.len()) && longest.is_some_and(|len| len <= 60) {
        return "grid".to_owned();
    }
    // ponytail: no rule beyond this. The remaining layouts (`split`, `chart`)
    // need a visual to fill their other half — inventing one here would leave a
    // blank column, which reads as broken rather than designed.
    "content".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::create_document::theme;
    use serde_json::from_value;

    #[test]
    fn authored_elements_ride_through_to_the_renderer() {
        let slide: Slide = serde_json::from_value(json!({
            "title": "Custom",
            "layout": "blank",
            "elements": [
                {"kind": "text", "x": 1.0, "y": 2.0, "w": 5.0, "h": 1.0, "text": "Hi"},
                {"kind": "shape", "x": 0.0, "y": 0.0, "w": 13.3, "h": 0.4, "fill": "112233"}
            ]
        }))
        .unwrap();
        let out = slide_json(&slide, 0);
        assert_eq!(
            out["layout"], "blank",
            "an explicit blank layout is honored"
        );
        assert_eq!(out["elements"].as_array().unwrap().len(), 2);
        assert_eq!(out["elements"][0]["text"], "Hi");
    }

    #[test]
    fn a_slide_without_elements_emits_no_elements_key() {
        let slide: Slide =
            serde_json::from_value(json!({"title": "Plain", "bullets": ["a"]})).unwrap();
        assert!(slide_json(&slide, 1).get("elements").is_none());
    }

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
            "slides": [
                {"title": "A"},
                {"title": "B", "layout": "hexagon", "bullets": [LONG_BULLET]}
            ]
        }))
        .unwrap();
        let deck = build_spec(&spec, &theme::resolve(None, "s"));
        assert_eq!(deck["slides"][1]["layout"], "content");
    }

    /// A deck the model gave no layouts to — the common case — must not come out
    /// as a cover plus a wall of identical bullet slides.
    #[test]
    fn an_unhinted_deck_still_varies_slide_to_slide() {
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [
                {"title": "Cover"},
                {"title": "Part One"},
                {"title": "Three points", "bullets": ["Faster", "Cheaper", "Safer"]},
                {"title": "The argument", "bullets": [LONG_BULLET, LONG_BULLET]}
            ]
        }))
        .unwrap();
        let deck = build_spec(&spec, &theme::resolve(None, "s"));
        let layouts: Vec<_> = deck["slides"]
            .as_array()
            .unwrap()
            .iter()
            .map(|slide| slide["layout"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(layouts, ["cover", "section", "grid", "content"]);
    }

    /// Prose belongs in `content`, which has the width for it — only short,
    /// parallel points become cards.
    const LONG_BULLET: &str = "A bullet long enough to be prose rather than a label, which a card grid \
         would crush into an unreadable box.";
}
