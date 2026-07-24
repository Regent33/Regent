//! Per-format validation for a `DocumentSpec`: confirm the array that drives the
//! requested `format` is present, and — for PPTX — that every `layout` hint is
//! one the deck renderer understands. A hallucinated diagram-only value like
//! `layout: "compare"` (paired with `items`/`points`, which
//! `#[serde(deny_unknown_fields)]` already rejects) thus fails with a sentence
//! instead of silently degrading to title-only slides. Kept apart from `model`
//! so the struct definitions stay under the file-size rule.

use super::model::{DocFormat, DocumentSpec, PPTX_LAYOUTS};

impl DocumentSpec {
    /// Confirms the spec that drives this format is actually present and, for a
    /// deck, that every layout hint is one the renderer understands. Returns a
    /// descriptive message (surfaced to the model as a tool error) otherwise.
    pub fn validate(&self) -> Result<(), String> {
        match self.format {
            DocFormat::Pdf | DocFormat::Docx => {
                if self.sections.is_empty() && self.title.is_none() {
                    return Err(format!(
                        "format '{}' needs `sections` (or at least a `title`); none were provided",
                        self.format.as_str()
                    ));
                }
            }
            DocFormat::Pptx => {
                if self.slides.is_empty() {
                    return Err(
                        "format 'pptx' needs a non-empty `slides` array (each with a `title`)"
                            .to_owned(),
                    );
                }
                for slide in &self.slides {
                    if let Some(layout) = &slide.layout {
                        let hint = layout.trim().to_ascii_lowercase();
                        if !PPTX_LAYOUTS.contains(&hint.as_str()) {
                            return Err(format!(
                                "slide '{}' has unknown layout '{layout}'; valid PPTX layouts are: \
                                 {}. Use `grid` plus bullets for a comparison inside a deck.",
                                slide.title,
                                PPTX_LAYOUTS.join(", "),
                            ));
                        }
                    }
                }
            }
            DocFormat::Xlsx => {
                if self.sheets.is_empty() {
                    return Err(
                        "format 'xlsx' needs a non-empty `sheets` array (each with `name` + `rows`)"
                            .to_owned(),
                    );
                }
                if let Some(empty) = self.sheets.iter().find(|s| s.rows.is_empty()) {
                    return Err(format!("sheet '{}' has no `rows`", empty.name));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json};

    #[test]
    fn a_known_pptx_layout_validates() {
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Cover"}, {"title": "Points", "layout": "grid"}]
        }))
        .unwrap();
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn a_bogus_pptx_layout_is_named_in_the_error() {
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Ferrari vs Lamborghini", "layout": "compare"}]
        }))
        .unwrap();
        let err = spec.validate().unwrap_err();
        assert!(err.contains("compare"), "layout value must be named: {err}");
        assert!(err.contains("Ferrari"), "slide title must be named: {err}");
    }

    /// `#[serde(deny_unknown_fields)]` on `Slide` stops the exact incident shape
    /// (`items`/`points` on a slide) at deserialization, before validation.
    #[test]
    fn diagram_items_on_a_slide_fail_to_deserialize() {
        let result: Result<DocumentSpec, _> = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "T", "items": [{"name": "A", "points": ["x"]}]}]
        }));
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("items"),
            "unknown slide field must error: {err}"
        );
    }
}
