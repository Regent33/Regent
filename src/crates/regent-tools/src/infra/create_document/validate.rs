//! Per-format validation for a `DocumentSpec`: confirm the array that drives the
//! requested `format` is present, and — for PPTX — that every `layout` hint is
//! one the deck renderer understands. A hallucinated diagram-only value like
//! `layout: "compare"` (paired with `items`/`points`, which
//! `#[serde(deny_unknown_fields)]` already rejects) thus fails with a sentence
//! instead of silently degrading to title-only slides. Kept apart from `model`
//! so the struct definitions stay under the file-size rule.

use super::model::{CHART_KINDS, DocFormat, DocumentSpec, PPTX_LAYOUTS, Slide, Table};

/// What fits on one 16:9 slide at a readable size.
///
/// Unenforced, a model hands over 35 bullets and 1,000 characters for a single
/// slide — measured, not guessed, on a deck the owner reported as unusable —
/// and every renderer then does the only thing it can with a fixed-height text
/// box: overflow it. The lines overlap and the slide is a wall of text. No
/// layout engine can rescue content that was the wrong shape before rendering
/// started, so it is refused here, where the model can still fix it.
const MAX_BULLETS: usize = 10;
/// `grid` draws each bullet as a card in two columns, so its ceiling is lower
/// than a bullet list's and is geometric rather than a matter of taste: cards
/// are at least 0.9in tall with 0.4in gaps below a 2.2in heading, so six (three
/// rows) is the most that fits on a 7.5in slide. A 13-item grid ran off the
/// bottom of the slide entirely.
const MAX_GRID_ITEMS: usize = 6;
const MAX_BODY_CHARS: usize = 700;
/// One bullet is a line, not a paragraph. The same deck had a 406-character
/// "bullet" — prose that belongs in `notes`, or in its own slide.
const MAX_BULLET_CHARS: usize = 200;

/// What fits in a table before it stops being readable. A slide is tighter than
/// a page: eight rows is already the whole body area.
const MAX_TABLE_COLUMNS: usize = 8;
const MAX_SLIDE_TABLE_ROWS: usize = 8;

/// Below this, a document is short enough that plain prose is a fair choice.
/// At or above it, text-only output is the "every document looks the same"
/// failure — and it is what a model produces by default, because every visual
/// field is optional and omitting them is always the least work. A 24-slide
/// deck and an 18-section report arrived with no image, table, chart or placed
/// element between them.
const VISUALS_REQUIRED_FROM: usize = 5;

/// Whether anything on this slide is not a line of text.
fn slide_has_visual(slide: &Slide) -> bool {
    slide.image.is_some()
        || slide.table.is_some()
        || slide.chart.is_some()
        || !slide.elements.is_empty()
}

/// Table shape problems, phrased as instructions. `None` when it is fine.
fn table_problem(table: &Table, slide_bound: bool) -> Option<String> {
    if table.rows.is_empty() {
        return Some("has a table with no `rows`".to_owned());
    }
    let columns = table.columns();
    if columns > MAX_TABLE_COLUMNS {
        return Some(format!(
            "has a {columns}-column table (max {MAX_TABLE_COLUMNS}) — drop columns, or turn the \
             table on its side so the long axis is rows"
        ));
    }
    if slide_bound && table.rows.len() > MAX_SLIDE_TABLE_ROWS {
        return Some(format!(
            "has a {}-row table on one slide (max {MAX_SLIDE_TABLE_ROWS}) — split it across \
             slides, or put the full table in a document instead",
            table.rows.len()
        ));
    }
    None
}

/// The density problem with one slide, phrased as an instruction. `None` when
/// the slide is fine.
fn density_problem(slide: &Slide) -> Option<String> {
    if let Some(chart) = &slide.chart {
        if !CHART_KINDS.contains(&chart.kind.as_str()) {
            return Some(format!(
                "has an unknown chart kind '{}' — valid kinds are: {}",
                chart.kind,
                CHART_KINDS.join(", ")
            ));
        }
        // A series whose labels and values disagree draws a chart that silently
        // omits points, which is worse than refusing to draw one.
        if let Some(bad) = chart
            .series
            .iter()
            .find(|s| s.labels.len() != s.values.len())
        {
            return Some(format!(
                "has a chart series '{}' with {} labels but {} values — they must match",
                bad.name,
                bad.labels.len(),
                bad.values.len()
            ));
        }
        if chart.series.iter().all(|s| s.values.is_empty()) {
            return Some("has a chart with no data".to_owned());
        }
    }
    if let Some(table) = &slide.table
        && let Some(problem) = table_problem(table, true)
    {
        return Some(problem);
    }
    let body: usize = slide.bullets.iter().map(|b| b.chars().count()).sum();
    let count = slide.bullets.len();
    let is_grid = slide
        .layout
        .as_deref()
        .is_some_and(|l| l.trim().eq_ignore_ascii_case("grid"));
    if is_grid && count > MAX_GRID_ITEMS {
        return Some(format!(
            "is a `grid` of {count} cards (max {MAX_GRID_ITEMS}) — they would run off the bottom \
             of the slide. Split it, or drop `layout: \"grid\"` to use a plain bullet list"
        ));
    }
    if count > MAX_BULLETS {
        return Some(format!(
            "has {count} bullets (max {MAX_BULLETS}) — split it across {} slides",
            count.div_ceil(MAX_BULLETS)
        ));
    }
    if body > MAX_BODY_CHARS {
        return Some(format!(
            "carries {body} characters of bullets (max {MAX_BODY_CHARS}) — split it, or move the \
             detail into `notes`"
        ));
    }
    if let Some(long) = slide
        .bullets
        .iter()
        .find(|b| b.chars().count() > MAX_BULLET_CHARS)
    {
        return Some(format!(
            "has a {}-character bullet (max {MAX_BULLET_CHARS}) starting \"{}…\" — a bullet is a \
             line, not a paragraph; shorten it or move it into `notes`",
            long.chars().count(),
            long.chars().take(40).collect::<String>(),
        ));
    }
    None
}

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
                for section in &self.sections {
                    if let Some(table) = &section.table
                        && let Some(problem) = table_problem(table, false)
                    {
                        return Err(format!(
                            "section '{}' {problem}",
                            section.heading.as_deref().unwrap_or("(untitled)")
                        ));
                    }
                }
                // `html` is the model designing the whole page itself, which is
                // more than this asks for.
                let designed = self.html.as_deref().is_some_and(|h| !h.trim().is_empty());
                let has_visual = self
                    .sections
                    .iter()
                    .any(|s| s.image.is_some() || s.table.is_some());
                if !designed && !has_visual && self.sections.len() >= VISUALS_REQUIRED_FROM {
                    return Err(format!(
                        "this is {} sections of unbroken prose with no figure or table anywhere. \
                         Add at least one: `table` for anything tabular, or `image` with a \
                         `query` like {{\"query\": \"tim burton portrait\"}} — a real photo is \
                         fetched for you, no key needed. Then retry.",
                        self.sections.len()
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
                // Every offender at once, not the first: reporting one at a
                // time costs the model a whole round trip per slide, and a
                // deck that is too dense is usually too dense throughout.
                let crowded: Vec<String> = self
                    .slides
                    .iter()
                    .filter_map(|slide| {
                        density_problem(slide).map(|why| format!("'{}' {why}", slide.title))
                    })
                    .collect();
                if self.slides.len() >= VISUALS_REQUIRED_FROM
                    && !self.slides.iter().any(slide_has_visual)
                {
                    return Err(format!(
                        "this is {} slides of bullet lists with nothing visual on any of them — \
                         the one outcome to avoid. Add at least one `image` (a `query` like \
                         {{\"query\": \"tim burton portrait\"}} fetches a real photo, no key \
                         needed), `table`, `chart`, or `elements` block, then retry.",
                        self.slides.len()
                    ));
                }
                if !crowded.is_empty() {
                    return Err(format!(
                        "{} slide(s) hold more than fits and would render as overlapping text. \
                         Fix them all now, in this turn, then retry: {}",
                        crowded.len(),
                        crowded.join("; "),
                    ));
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

    /// The measured shape of the deck the owner reported: 28 bullets on one
    /// slide, which overlapped into an unreadable block.
    #[test]
    fn an_overcrowded_slide_is_refused_with_instructions() {
        let bullets: Vec<String> = (0..28).map(|i| format!("point {i}")).collect();
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Awards & Recognition", "bullets": bullets}]
        }))
        .unwrap();
        let err = spec.validate().unwrap_err();
        assert!(err.contains("Awards & Recognition"), "names the slide: {err}");
        assert!(err.contains("28 bullets"), "names the count: {err}");
        assert!(err.contains("split"), "says what to do: {err}");
    }

    /// The measured shape of a real run: 24 slides and 18 sections with no
    /// image, table, chart or element anywhere. Every visual field is optional,
    /// so omitting all of them is always the least work — which is exactly how
    /// every document ends up looking the same.
    #[test]
    fn a_long_text_only_document_is_refused() {
        let slides: Vec<_> = (0..24)
            .map(|i| json!({"title": format!("Slide {i}"), "bullets": ["a", "b"]}))
            .collect();
        let deck: DocumentSpec = from_value(json!({"format": "pptx", "slides": slides})).unwrap();
        let err = deck.validate().unwrap_err();
        assert!(err.contains("24 slides"), "names the size: {err}");
        assert!(err.contains("query"), "offers the cheapest fix: {err}");

        let sections: Vec<_> = (0..18)
            .map(|i| json!({"heading": format!("S{i}"), "paragraphs": ["text"]}))
            .collect();
        let doc: DocumentSpec =
            from_value(json!({"format": "pdf", "sections": sections})).unwrap();
        assert!(doc.validate().unwrap_err().contains("18 sections"));
    }

    #[test]
    fn one_visual_anywhere_is_enough_and_short_documents_are_exempt() {
        let mut slides: Vec<_> = (0..24)
            .map(|i| json!({"title": format!("Slide {i}"), "bullets": ["a"]}))
            .collect();
        slides[7] = json!({"title": "Cast", "table": {"headers": ["a"], "rows": [["b"]]}});
        let deck: DocumentSpec = from_value(json!({"format": "pptx", "slides": slides})).unwrap();
        assert!(deck.validate().is_ok(), "one table is enough");

        // A four-slide deck is short enough that plain text is a fair choice.
        let short: Vec<_> = (0..4)
            .map(|i| json!({"title": format!("S{i}"), "bullets": ["a"]}))
            .collect();
        let brief: DocumentSpec = from_value(json!({"format": "pptx", "slides": short})).unwrap();
        assert!(brief.validate().is_ok());

        // Authored HTML is the model designing the page itself — more than the
        // rule asks for, so it does not also demand a figure.
        let sections: Vec<_> = (0..18).map(|i| json!({"heading": format!("S{i}")})).collect();
        let authored: DocumentSpec = from_value(json!({
            "format": "pdf", "sections": sections, "html": "<h1>Designed</h1>"
        }))
        .unwrap();
        assert!(authored.validate().is_ok());
    }

    /// A 13-card grid ran clean off the bottom of the slide. The cards have a
    /// minimum height, so this ceiling is geometry, not taste — and it is
    /// lower than the plain-bullet one.
    #[test]
    fn a_grid_is_capped_lower_than_a_bullet_list() {
        let items: Vec<String> = (0..13).map(|i| format!("cast member {i}")).collect();
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Main Cast", "layout": "grid", "bullets": items}]
        }))
        .unwrap();
        let err = spec.validate().unwrap_err();
        assert!(err.contains("13 cards"), "names the count: {err}");
        assert!(err.contains("run off the bottom"), "says why: {err}");

        // The same 13 as a plain list is judged by the bullet rule instead.
        let plain: Vec<String> = (0..13).map(|i| format!("cast member {i}")).collect();
        let list: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Main Cast", "bullets": plain}]
        }))
        .unwrap();
        assert!(list.validate().unwrap_err().contains("13 bullets"));

        // Six fits.
        let ok: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Agenda", "layout": "grid",
                        "bullets": ["a", "b", "c", "d", "e", "f"]}]
        }))
        .unwrap();
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn every_crowded_slide_is_reported_at_once() {
        let many: Vec<String> = (0..20).map(|i| format!("p{i}")).collect();
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [
                {"title": "Fine", "bullets": ["a", "b"]},
                {"title": "First", "bullets": many},
                {"title": "Second", "bullets": (0..30).map(|i| format!("q{i}")).collect::<Vec<_>>()},
            ]
        }))
        .unwrap();
        let err = spec.validate().unwrap_err();
        assert!(err.contains("First") && err.contains("Second"), "both named: {err}");
        assert!(!err.contains("Fine"), "the good slide is not blamed: {err}");
    }

    #[test]
    fn a_paragraph_masquerading_as_a_bullet_is_refused() {
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Synopsis", "bullets": ["x".repeat(406)]}]
        }))
        .unwrap();
        let err = spec.validate().unwrap_err();
        assert!(err.contains("406-character"), "names the length: {err}");
        assert!(err.contains("notes"), "offers the alternative: {err}");
    }

    /// The limits must not refuse an ordinary, well-shaped deck.
    #[test]
    fn a_normal_deck_still_passes() {
        let spec: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [
                {"title": "Cover", "subtitle": "A deck"},
                {"title": "Findings", "bullets": [
                    "Revenue grew 24% year over year",
                    "Churn fell to 3.1%, the lowest since 2024",
                    "Two enterprise accounts renewed early",
                ]},
            ]
        }))
        .unwrap();
        assert!(spec.validate().is_ok());
    }

    /// `chart` was a valid layout with nowhere to put the data, so asking for
    /// one produced a slide with a heading and nothing under it.
    #[test]
    fn a_chart_reaches_the_deck_and_its_shape_is_checked() {
        let good = json!({
            "format": "pptx",
            "slides": [{"title": "Growth", "layout": "chart", "chart": {
                "kind": "bar",
                "series": [{"name": "Revenue", "labels": ["Q1", "Q2"], "values": [1.0, 2.0]}]
            }}]
        });
        let spec: DocumentSpec = from_value(good).unwrap();
        assert!(spec.validate().is_ok());

        let mismatched: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Growth", "chart": {
                "kind": "bar",
                "series": [{"name": "Revenue", "labels": ["Q1", "Q2"], "values": [1.0]}]
            }}]
        }))
        .unwrap();
        let err = mismatched.validate().unwrap_err();
        assert!(err.contains("2 labels but 1 values"), "{err}");

        let unknown: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "G", "chart": {"kind": "donut", "series": []}}]
        }))
        .unwrap();
        assert!(unknown.validate().unwrap_err().contains("donut"));
    }

    #[test]
    fn a_table_is_accepted_on_a_section_and_a_slide() {
        let table = json!({"headers": ["Year", "Box office"], "rows": [["2018", "$1.35B"]]});
        let doc: DocumentSpec = from_value(json!({
            "format": "docx",
            "sections": [{"heading": "Numbers", "table": table}]
        }))
        .unwrap();
        assert!(doc.validate().is_ok());
        let deck: DocumentSpec = from_value(json!({
            "format": "pptx",
            "slides": [{"title": "Numbers", "table": table}]
        }))
        .unwrap();
        assert!(deck.validate().is_ok());
    }

    #[test]
    fn an_oversized_table_is_refused_with_the_reason() {
        let wide: Vec<String> = (0..12).map(|i| format!("c{i}")).collect();
        let spec: DocumentSpec = from_value(json!({
            "format": "pdf",
            "sections": [{"heading": "Grid", "table": {"headers": wide, "rows": [["a"]]}}]
        }))
        .unwrap();
        let err = spec.validate().unwrap_err();
        assert!(err.contains("12-column"), "names the width: {err}");

        // A slide is tighter than a page, so rows are capped there and not here.
        let rows: Vec<Vec<String>> = (0..20).map(|i| vec![format!("r{i}")]).collect();
        let long = json!({"headers": ["x"], "rows": rows});
        let page: DocumentSpec =
            from_value(json!({"format": "pdf", "sections": [{"table": long}]})).unwrap();
        assert!(page.validate().is_ok(), "20 rows is fine on a page");
        let deck: DocumentSpec =
            from_value(json!({"format": "pptx", "slides": [{"title": "T", "table": long}]}))
                .unwrap();
        assert!(deck.validate().unwrap_err().contains("20-row"));
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
