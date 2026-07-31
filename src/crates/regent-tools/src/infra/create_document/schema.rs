//! The `create_document` tool definition — the JSON schema and description the
//! model sees. Kept apart from the executor so the (large) schema doesn't crowd
//! the run logic.

use regent_kernel::ToolDefinition;
use serde_json::json;

/// The `slides` array schema — its own function because inlining it hit
/// `json!`'s recursion limit once slides gained charts and tables. Splitting
/// on the array boundary keeps each macro shallow and each block readable.
fn slides_schema() -> serde_json::Value {
    json!({
                    "type": "array",
                    "description": "Slides — drive pptx. DESIGN the deck, do not dump the research \
                                    into it. Hard limits, enforced: at most 10 bullets and 700 \
                                    characters of bullets per slide, and no bullet over 200 \
                                    characters — a slide that exceeds them is REFUSED, because it \
                                    renders as overlapping text. Split dense material across more \
                                    slides (a 25-line topic is three slides, not one), put prose in \
                                    `notes`, vary `layout` slide to slide, and reach for `image` \
                                    and `elements` — a deck of identical bullet lists is the one \
                                    outcome to avoid.",
                    "items": {"type": "object", "properties": {
                        "title": {"type": "string"},
                        "subtitle": {"type": "string"},
                        "bullets": {"type": "array", "items": {"type": "string"}},
                        "notes": {"type": "string"},
                        "layout": {
                            "type": "string",
                            "enum": ["cover", "content", "section", "split", "chart", "grid", "blank"],
                            "description": "Optional per-slide layout; omitted → chosen from the slide's content. \
                                            `grid` renders the slide's bullets as numbered cards (agenda/overview look)."
                        },
                        "chart": {
                            "type": "object",
                            "description": "A native, editable PowerPoint chart. Whenever the point \
                                            is a trend, a share, or a comparison of numbers, this \
                                            beats listing the numbers as bullets. Pairs with \
                                            `layout: \"chart\"` (full width) or `split`/`content` \
                                            (beside the text).",
                            "properties": {
                                "kind": {"type": "string", "enum": ["bar", "line", "pie"]},
                                "series": {
                                    "type": "array",
                                    "description": "One entry per series; `labels` and `values` must be the same length.",
                                    "items": {"type": "object", "properties": {
                                        "name": {"type": "string"},
                                        "labels": {"type": "array", "items": {"type": "string"}},
                                        "values": {"type": "array", "items": {"type": "number"}}
                                    }, "required": ["labels", "values"]}
                                }
                            },
                            "required": ["kind", "series"]
                        },
                        "table": {
                            "type": "object",
                            "description": "A real PowerPoint table on this slide. Max 8 columns \
                                            and 8 rows — a slide is not a spreadsheet; a longer \
                                            table belongs in a document. Bullets may sit above it \
                                            as a lead-in.",
                            "properties": {
                                "headers": {"type": "array", "items": {"type": "string"}},
                                "rows": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}},
                                "caption": {"type": "string"}
                            },
                            "required": ["rows"]
                        },
                        "elements": {
                            "type": "array",
                            "description": "Model-placed elements — the deck's design escape hatch. Each is \
                                            {kind: 'text'|'shape'|'image', x, y, w, h, ...} in INCHES on a \
                                            13.33x7.5 slide. Pair with layout 'blank' to compose a slide \
                                            yourself, the way you would lay out a real deck; or add them on \
                                            top of a named layout to decorate it. text: text/fontSize/ \
                                            fontFace/color/bold/italic/align/valign. shape: fill/line/rounded. \
                                            image: imageBase64. Omitted colours and fonts fall back to the \
                                            deck theme. Max 60 elements per slide.",
                            "items": {"type": "object"}
                        },
                        "image": {
                            "type": "object",
                            "description": "Optional visual for this slide, sourced by ONE of (checked in order): \
                                            `query` — a few search words; a matching, commercially-licensed photo \
                                            is fetched keylessly and embedded (the easiest way to illustrate a \
                                            slide, no image key needed); `url` — a direct image link; or `path` — \
                                            a local PNG/JPEG you already have. Add `alt_text` for accessibility. A \
                                            source that can't be resolved is skipped and reported in `image_notes`, \
                                            never fatal.",
                            "properties": {
                                "query": {"type": "string", "description": "Search words, e.g. 'stanford campus autumn'. A relevant photo is downloaded and embedded automatically."},
                                "url": {"type": "string", "description": "Direct link to an image to download and embed."},
                                "path": {"type": "string", "description": "Local PNG/JPEG path (e.g. one you generated with image_generation or extracted via read_document)."},
                                "alt_text": {"type": "string"}
                            }
                        }
                    }, "required": ["title"]}
                })
}

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "create_document".into(),
        description: "Create or edit a PDF/Word/PowerPoint/Excel file. \
                      \
                      YOU are the designer — the fields below are a medium, not a form to fill in. \
                      Reach for the right one: `table` for anything tabular (PDF, Word AND slides — \
                      never flatten rows into bullets), `chart` for a trend or a comparison of \
                      numbers, `image` (a `query` fetches a real photo, no key needed), and a \
                      `layout` chosen per slide. When the SHAPE of the thing matters, take the \
                      whole canvas: `html` on a PDF is a complete document you write yourself, and \
                      `elements` with `layout: \"blank\"` is a slide you compose shape by shape in \
                      inches. A document that is nothing but title-and-bullets is a failure of \
                      imagination, not a limitation of this tool. \
                      \
                      Density is enforced, not advised: a slide over 10 bullets / 700 characters, a \
                      `grid` over 6 cards, or a bullet over 200 characters is REFUSED, because it \
                      renders as overlapping text. Split it rather than cramming it. \
                      \
                      A relative path saves under the artifacts directory, and everything you \
                      create in one conversation is grouped in one folder automatically. Pass an \
                      absolute path only when the user names a location. To revise a file this tool \
                      made, pass `operation: \"edit\"` with the same `path` and a `patch` of just \
                      the fields to change."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "format": {"type": "string", "enum": ["pdf", "docx", "pptx", "xlsx"]},
                "path": {"type": "string", "description": "Output file path. For an edit, the path returned when the file was created."},
                "operation": {
                    "type": "string",
                    "enum": ["create", "edit"],
                    "description": "Defaults to 'create'. 'edit' reloads the saved spec of a file this tool made and applies `patch`."
                },
                "patch": {
                    "type": "object",
                    "description": "Edit only: a JSON merge-patch over the saved spec. Set a field to change it, or to null to remove it; arrays replace wholesale (resupply the full slides/sections/sheets array to change one)."
                },
                "preview": {
                    "type": "boolean",
                    "description": "When true, also render a background <file>.preview.png (headless — no window) so you can `vision_analyze` it and fix layout/contrast/overflow, then edit. PDF screenshots the report; PPTX needs LibreOffice installed (else a note is returned)."
                },
                "title": {"type": "string"},
                "theme": {
                    "description": "Optional look, applied to all four formats. Either a preset name \
                                    (midnight | warm-editorial | mono | forest | royal) or a custom \
                                    palette object with any of: background, text, accent, muted, \
                                    coverBackground, coverText, titleFont, bodyFont (6-hex colors, \
                                    no '#'), plus optional `base` preset to inherit from. Omitted → \
                                    a palette is generated from the content (unique per document, \
                                    contrast-checked), so leaving it off is safe; set it when the \
                                    subject calls for a specific mood."
                },
                "sections": {
                    "type": "array",
                    "description": "Prose blocks — drive pdf/docx.",
                    "items": {"type": "object", "properties": {
                        "heading": {"type": "string"},
                        "paragraphs": {"type": "array", "items": {"type": "string"}},
                        "bullets": {"type": "array", "items": {"type": "string"}},
                        "table": {
                            "type": "object",
                            "description": "A real table — a Word table in DOCX, a laid-out table \
                                            in PDF. Use it for ANYTHING tabular (comparisons, \
                                            figures, specs, schedules); flattening rows into \
                                            bullets is what makes a document unreadable. Max 8 \
                                            columns.",
                            "properties": {
                                "headers": {"type": "array", "items": {"type": "string"}, "description": "Column headers; omit for a table with no header row."},
                                "rows": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}, "description": "Row cells as strings — format numbers the way you want them shown."},
                                "caption": {"type": "string"}
                            },
                            "required": ["rows"]
                        },
                        "image": {
                            "type": "object",
                            "description": "Optional figure for this section (PDF only), sourced by ONE of \
                                            `query` (keyless photo lookup), `url`, or `path` — same as slide \
                                            images. Add `alt_text`. Unresolvable sources land in `image_notes`.",
                            "properties": {
                                "query": {"type": "string"},
                                "url": {"type": "string"},
                                "path": {"type": "string"},
                                "alt_text": {"type": "string"}
                            }
                        }
                    }}
                },
                "html": {
                    "type": "string",
                    "description": "PDF only. A complete HTML document (inline CSS; a bare fragment is                                     wrapped for you) rendered INSTEAD of the built-in report template.                                     Use it whenever the document's shape is part of the point — pitch                                     decks, invoices, resumes, posters, one-pagers — and design it as                                     you would a real web page: your own grid, type scale, and colour.                                     Print rules apply (@page, page-break-inside: avoid). Still send                                     `sections` alongside: they are what a browserless fallback renders                                     and what a later `operation: \"edit\"` merges against."
                },
                "slides": slides_schema(),
                "sheets": {
                    "type": "array",
                    "description": "Worksheets — drive xlsx.",
                    "items": {"type": "object", "properties": {
                        "name": {"type": "string"},
                        "rows": {"type": "array", "items": {"type": "array"}},
                        "header": {"type": "boolean"}
                    }, "required": ["name", "rows"]}
                }
            },
            "required": ["format", "path"]
        }),
        toolset: "documents".into(),
    }
}
