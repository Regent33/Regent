//! Content spec for `create_document`: one flat struct the model fills, plus
//! per-format validation so a mismatched request (a deck with only `sheets`)
//! fails with a sentence, not a blank file.

use serde::Deserialize;
use serde_json::Value;

/// The four office formats we can synthesize in-process.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocFormat {
    Pdf,
    Docx,
    Pptx,
    Xlsx,
}

impl DocFormat {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DocFormat::Pdf => "pdf",
            DocFormat::Docx => "docx",
            DocFormat::Pptx => "pptx",
            DocFormat::Xlsx => "xlsx",
        }
    }
}

/// One prose block: an optional heading, body paragraphs, and bullets. Drives
/// PDF and DOCX.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct Section {
    #[serde(default)]
    pub heading: Option<String>,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub bullets: Vec<String>,
}

/// Optional visual asset for a slide. The path is resolved through the same
/// filesystem jail as every other file tool before its bytes enter the deck.
#[derive(Debug, Deserialize, Clone)]
pub struct SlideImage {
    pub path: String,
    #[serde(default)]
    pub alt_text: Option<String>,
}

/// Image bytes hydrated by the executor after sandbox resolution. This field
/// never comes from JSON; the PPTX writer consumes it directly.
#[derive(Debug, Clone)]
pub struct EmbeddedSlideImage {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
    pub content_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub alt_text: String,
}

/// One slide: a claim-led title, optional subtitle, concise bullet body,
/// optional visual, and speaker notes. Drives PPTX.
#[derive(Debug, Deserialize, Clone)]
pub struct Slide {
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub bullets: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub image: Option<SlideImage>,
    #[serde(skip)]
    pub embedded_image: Option<EmbeddedSlideImage>,
}

/// One worksheet: a name and a grid of string/number cells. Drives XLSX.
#[derive(Debug, Deserialize, Clone)]
pub struct Sheet {
    pub name: String,
    #[serde(default)]
    pub rows: Vec<Vec<Value>>,
    /// When true, the first row is emitted bold (a header row).
    #[serde(default)]
    pub header: bool,
}

/// The whole request. `format` selects which of `sections` / `slides` /
/// `sheets` is authoritative; the rest are ignored.
#[derive(Debug, Deserialize, Clone)]
pub struct DocumentSpec {
    pub format: DocFormat,
    pub path: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub slides: Vec<Slide>,
    #[serde(default)]
    pub sheets: Vec<Sheet>,
}

impl DocumentSpec {
    /// Confirms the spec that drives this format is actually present. Returns a
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
