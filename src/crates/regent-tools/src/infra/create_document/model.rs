//! Content spec for `create_document`: one flat struct the model fills, plus
//! per-format validation so a mismatched request (a deck with only `sheets`)
//! fails with a sentence, not a blank file.

use super::theme::ThemeChoice;
use serde::{Deserialize, Serialize};
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

/// One prose block: an optional heading, body paragraphs, bullets, and an
/// optional image. Drives PDF and DOCX. Serializes so the HTML report template
/// can loop over it — `image` (the source) is stripped; `image_render` (the
/// hydrated data URI) is what the template reads.
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Section {
    #[serde(default)]
    pub heading: Option<String>,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub bullets: Vec<String>,
    /// Image source from the request; resolved into `image_render` at hydrate
    /// time (PDF only). Not serialized to the template.
    #[serde(default, skip_serializing)]
    pub image: Option<ImageSource>,
    /// The hydrated image, ready for the HTML template. Never comes from JSON.
    #[serde(default, skip_deserializing)]
    pub image_render: Option<RenderedImage>,
}

/// A hydrated image the HTML report template embeds inline (data URI + alt).
#[derive(Debug, Serialize, Clone)]
pub struct RenderedImage {
    pub data_uri: String,
    pub alt: String,
}

/// Optional visual asset for a slide or section, sourced one of three ways
/// (checked in this order): a local `path` (resolved through the same filesystem
/// jail as every other file tool), a direct `url`, or a `query` we look up
/// keylessly and download. `url`/`query` are best-effort — a miss becomes a
/// note, not a failure — so a document never sinks over one unavailable picture.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct ImageSource {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
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
    pub image: Option<ImageSource>,
    /// Optional layout hint for the renderer: cover | content | section | split
    /// | chart | grid | blank. Omitted → the deck builder picks one from the
    /// content. `grid` lays the bullets out as numbered cards.
    #[serde(default)]
    pub layout: Option<String>,
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
    #[serde(default)]
    pub title: Option<String>,
    /// Optional theme: a preset name OR a full custom palette (see `theme`).
    /// Omitted → a stable theme is derived from the content so different
    /// documents don't look identical.
    #[serde(default)]
    pub theme: Option<ThemeChoice>,
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
