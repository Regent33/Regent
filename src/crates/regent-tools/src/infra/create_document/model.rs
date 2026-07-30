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
#[serde(deny_unknown_fields)]
pub struct Section {
    #[serde(default)]
    pub heading: Option<String>,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub bullets: Vec<String>,
    /// A table for this section. Serialized, so the HTML report template loops
    /// over it the same way it loops over bullets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<Table>,
    /// Image source from the request; resolved into `image_render` at hydrate
    /// time (PDF only). Not serialized to the template.
    #[serde(default, skip_serializing)]
    pub image: Option<ImageSource>,
    /// The hydrated image, ready for the HTML template. Never comes from JSON.
    #[serde(default, skip_deserializing)]
    pub image_render: Option<RenderedImage>,
}

/// A table. The one content shape none of the four formats could express —
/// anything tabular had to be flattened into bullets, which is how a comparison
/// or a set of figures ended up as an unreadable list.
///
/// Rows are strings, not numbers: this is presentation, and a spec that tried
/// to be a data model would have to answer formatting questions ("2 decimal
/// places? a currency symbol?") that the model has already answered by writing
/// the string it wants shown.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// Column headers. May be empty for a plain grid with no header row.
    #[serde(default)]
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    #[serde(default)]
    pub caption: Option<String>,
}

impl Table {
    /// Columns the table actually occupies — the widest row, so a ragged table
    /// still renders every cell it was given rather than truncating.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.rows
            .iter()
            .map(Vec::len)
            .chain(std::iter::once(self.headers.len()))
            .max()
            .unwrap_or(0)
    }

    /// `rows`, each padded to `columns()`. Short rows are a normal thing for a
    /// model to emit and must not produce a torn table.
    #[must_use]
    pub fn padded_rows(&self) -> Vec<Vec<String>> {
        let width = self.columns();
        self.rows
            .iter()
            .map(|row| {
                let mut row = row.clone();
                row.resize(width, String::new());
                row
            })
            .collect()
    }
}

/// A native PowerPoint chart: bar, line or pie, one or more series.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Chart {
    /// bar | line | pie. Anything else is refused by `validate`.
    pub kind: String,
    pub series: Vec<ChartSeries>,
}

/// One series: a name, the category labels, and a value per label.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ChartSeries {
    #[serde(default)]
    pub name: String,
    pub labels: Vec<String>,
    pub values: Vec<f64>,
}

/// The chart kinds the renderer draws.
pub const CHART_KINDS: &[&str] = &["bar", "line", "pie"];

/// A hydrated image the HTML report template embeds inline (data URI + alt).
#[derive(Debug, Serialize, Clone)]
pub struct RenderedImage {
    pub data_uri: String,
    pub alt: String,
    /// The same image as raw bytes. The HTML template wants a data URI; Word
    /// wants the bytes themselves, so both live here rather than making DOCX
    /// parse the URI back apart. Never serialized — the template ignores it.
    #[serde(skip_serializing)]
    pub bytes: Vec<u8>,
    /// Pixel dimensions, carried from the hydrator so a writer that has to size
    /// the image (Word) never decodes it a second time to ask.
    #[serde(skip_serializing)]
    pub width: u32,
    #[serde(skip_serializing)]
    pub height: u32,
}

/// Optional visual asset for a slide or section, sourced one of three ways
/// (checked in this order): a local `path` (resolved through the same filesystem
/// jail as every other file tool), a direct `url`, or a `query` we look up
/// keylessly and download. `url`/`query` are best-effort — a miss becomes a
/// note, not a failure — so a document never sinks over one unavailable picture.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
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

/// The layout hints the PPTX deck renderer understands — one source of truth,
/// shared by [`DocumentSpec::validate`] (which rejects any other value up front,
/// so a hallucinated `layout: "compare"` fails with a sentence) and
/// `deck::slide_json` (which falls back to an auto layout as a last defense).
pub(crate) const PPTX_LAYOUTS: &[&str] = &[
    "cover", "content", "section", "split", "chart", "grid", "blank",
];

/// One slide: a claim-led title, optional subtitle, concise bullet body,
/// optional visual, and speaker notes. Drives PPTX.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
    /// Model-placed elements — the deck equivalent of the PDF's `html` escape
    /// hatch. Each is `{kind: text|shape|image, x, y, w, h, …}` in inches on a
    /// 13.33×7.5 slide, passed straight to the PptxGenJS renderer, so the model
    /// can compose a layout instead of picking one of seven recipes. Pair with
    /// `layout: "blank"` to own the whole slide; leave the layout alone to
    /// decorate on top of it.
    #[serde(default)]
    pub elements: Vec<serde_json::Value>,
    /// A table on this slide. Bullets and a table can coexist — a short lead-in
    /// above the figures is a normal slide.
    #[serde(default)]
    pub table: Option<Table>,
    /// A native PowerPoint chart. `chart` has been a valid `layout` since the
    /// renderer shipped, and the renderer has always known how to draw one —
    /// but there was no field to put the DATA in, so asking for that layout
    /// produced a slide with a heading and nothing else.
    #[serde(default)]
    pub chart: Option<Chart>,
    #[serde(skip)]
    pub embedded_image: Option<EmbeddedSlideImage>,
}

/// One worksheet: a name and a grid of string/number cells. Drives XLSX.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
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
    /// PDF only: a complete HTML document to render INSTEAD of the built-in
    /// report template.
    ///
    /// The template gives every PDF the same skeleton — cover, then sections
    /// with an accent-underlined heading and dot bullets — and varies only its
    /// palette. That's right for a quick report and wrong for everything whose
    /// shape IS the point: a pitch deck, an invoice, a résumé, a one-pager. So
    /// the model can hand over real markup and own the layout, exactly the way
    /// it would if it were writing a web page.
    ///
    /// `sections` is still worth sending alongside: it's what a fallback
    /// native render (no browser present) uses, and what `operation:"edit"`
    /// merges against.
    #[serde(default)]
    pub html: Option<String>,
}

// `DocumentSpec::validate` (per-format presence + PPTX layout checks) lives in
// `validate.rs` to keep this schema-definition file under the file-size rule.
