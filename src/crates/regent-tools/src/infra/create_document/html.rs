//! Themed report HTML for the PDF path. Tera renders one self-contained document
//! (inlined CSS from the resolved `Theme`) that headless Chromium prints to PDF —
//! real fonts, color, and layout the native lopdf writer can't express. Tera
//! autoescapes every value, so model/user text can't break the markup.

use super::model::DocumentSpec;
use super::theme::Theme;
use tera::{Context, Tera};

const REPORT_TEMPLATE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<style>
  @page { size: A4; margin: 20mm 18mm; }
  * { box-sizing: border-box; }
  body { margin: 0; color: #{{ theme.text }}; background: #{{ theme.background }};
         font-family: "{{ theme.bodyFont }}", system-ui, sans-serif; line-height: 1.55; }
  .cover { background: #{{ theme.coverBackground }}; color: #{{ theme.coverText }};
           padding: 42px 40px; border-radius: 14px; margin-bottom: 30px; }
  .cover .kicker { text-transform: uppercase; letter-spacing: 3px; font-size: 11px;
                   color: #{{ theme.accent }}; font-weight: 700; margin: 0 0 12px; }
  .cover h1 { font-family: "{{ theme.titleFont }}", Georgia, serif; font-weight: 700;
              font-size: 40px; line-height: 1.12; margin: 0; }
  section { margin: 0 0 22px; page-break-inside: avoid; }
  h2 { font-family: "{{ theme.titleFont }}", Georgia, serif; font-size: 20px;
       color: #{{ theme.text }}; margin: 0 0 4px; padding-bottom: 6px;
       border-bottom: 2px solid #{{ theme.accent }}; }
  p { margin: 0 0 10px; }
  ul { margin: 8px 0 0; padding-left: 0; list-style: none; }
  li { position: relative; padding-left: 20px; margin: 0 0 7px; }
  li::before { content: ""; position: absolute; left: 0; top: 8px; width: 8px; height: 8px;
               border-radius: 50%; background: #{{ theme.accent }}; }
  .muted { color: #{{ theme.muted }}; }
  figure { margin: 14px 0 4px; }
  figure img { max-width: 100%; height: auto; border-radius: 10px; display: block; }
  /* `avoid` on the whole table splits a long one badly across pages; the header
     repeats instead, which is what a printed table is supposed to do. */
  table { width: 100%; border-collapse: collapse; margin: 12px 0 4px; font-size: 12px; }
  thead { display: table-header-group; }
  th { text-align: left; font-family: "{{ theme.titleFont }}", Georgia, serif;
       color: #{{ theme.accent }}; border-bottom: 2px solid #{{ theme.accent }};
       padding: 7px 9px; }
  td { padding: 6px 9px; border-bottom: 1px solid #{{ theme.muted }}33; }
  tr { page-break-inside: avoid; }
  caption { caption-side: bottom; text-align: left; padding-top: 6px;
            font-size: 11px; color: #{{ theme.muted }}; }
</style></head>
<body>
  {% if title %}<div class="cover"><p class="kicker">Regent</p><h1>{{ title }}</h1></div>{% endif %}
  {% for section in sections %}
  <section>
    {% if section.heading %}<h2>{{ section.heading }}</h2>{% endif %}
    {% for para in section.paragraphs %}<p>{{ para }}</p>{% endfor %}
    {% if section.image_render %}<figure><img src="{{ section.image_render.data_uri | safe }}" alt="{{ section.image_render.alt }}"></figure>{% endif %}
    {% if section.bullets %}<ul>{% for bullet in section.bullets %}<li>{{ bullet }}</li>{% endfor %}</ul>{% endif %}
    {% if section.table %}<table>
      {% if section.table.caption %}<caption>{{ section.table.caption }}</caption>{% endif %}
      {% if section.table.headers %}<thead><tr>{% for h in section.table.headers %}<th>{{ h }}</th>{% endfor %}</tr></thead>{% endif %}
      <tbody>{% for row in section.table.rows %}<tr>{% for cell in row %}<td>{{ cell }}</td>{% endfor %}</tr>{% endfor %}</tbody>
    </table>{% endif %}
  </section>
  {% endfor %}
</body></html>"#;

/// Render the themed report HTML for `spec`. Pure — no I/O.
pub fn report(spec: &DocumentSpec, theme: &Theme) -> Result<String, String> {
    let mut context = Context::new();
    context.insert("theme", theme);
    context.insert("title", &spec.title);
    context.insert("sections", &spec.sections);
    Tera::one_off(REPORT_TEMPLATE, &context, true)
        .map_err(|error| format!("report HTML render failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::create_document::theme;
    use serde_json::json;

    #[test]
    fn report_embeds_theme_and_escapes_content() {
        let spec: DocumentSpec = serde_json::from_value(json!({
            "format": "pdf", "title": "Q3 & Beyond",
            "sections": [{"heading": "Risks", "paragraphs": ["Watch <margins>."], "bullets": ["one"]}]
        }))
        .unwrap();
        let resolved = theme::resolve(Some(&theme::ThemeChoice::Named("royal".into())), "seed");
        let html = report(&spec, &resolved).unwrap();
        assert!(html.contains("#6D4AC4"), "theme accent missing: {html}");
        assert!(html.contains("Risks"), "heading missing");
        // Autoescape must neutralize markup from model/user text.
        assert!(html.contains("Q3 &amp; Beyond"), "title not escaped");
        assert!(html.contains("&lt;margins&gt;"), "paragraph not escaped");
        assert!(!html.contains("<margins>"), "raw markup leaked");
    }

    #[test]
    fn report_renders_a_section_table() {
        let spec: DocumentSpec = serde_json::from_value(json!({
            "format": "pdf", "title": "Numbers",
            "sections": [{"heading": "Box office", "table": {
                "headers": ["Film", "Gross"],
                "rows": [["Wakanda Forever", "$859M"]],
                "caption": "Worldwide"
            }}]
        }))
        .unwrap();
        let html = report(&spec, &theme::resolve(None, "t")).unwrap();
        assert!(html.contains("<table>"), "no table element: {html}");
        assert!(html.contains("<th>Film</th>"), "no header cell: {html}");
        assert!(html.contains("<td>$859M</td>"), "no body cell: {html}");
        assert!(html.contains("<caption>Worldwide</caption>"), "no caption");
    }

    #[test]
    fn report_embeds_a_section_image_when_hydrated() {
        use crate::infra::create_document::model::RenderedImage;
        let mut spec: DocumentSpec = serde_json::from_value(json!({
            "format": "pdf", "title": "Illustrated",
            "sections": [{"heading": "Visuals", "paragraphs": ["See below."]}]
        }))
        .unwrap();
        spec.sections[0].image_render = Some(RenderedImage {
            data_uri: "data:image/png;base64,ABCDEF".to_owned(),
            alt: "a chart".to_owned(),
            // The HTML path reads the data URI; the bytes and dimensions exist
            // for Word, which embeds the image itself.
            bytes: Vec::new(),
            width: 0,
            height: 0,
        });
        let html = report(&spec, &theme::resolve(None, "seed")).unwrap();
        assert!(
            html.contains("<img src=\"data:image/png;base64,ABCDEF\""),
            "section image not rendered: {html}"
        );
        assert!(html.contains("alt=\"a chart\""), "alt text missing");
    }
}
