//! Dependency-free YouTube web result resolver for `play`.
//!
//! The normal search page embeds `ytInitialData`, including `videoRenderer`
//! rows. Reading that public payload avoids making end users install Python or
//! yt-dlp merely to play a song. The optional yt-dlp resolver remains behind
//! this path as a compatibility fallback.

use serde_json::Value;
use std::time::Duration;

const SEARCH_TIMEOUT_SECS: u64 = 10;

pub(super) async fn resolve(query: &str) -> Option<(String, String)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(SEARCH_TIMEOUT_SECS))
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/126.0 Safari/537.36",
        )
        .build()
        .ok()?;
    let url = format!(
        "https://www.youtube.com/results?search_query={}&hl=en",
        super::url_encode(query)
    );
    let html = client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    pick_from_html(&html, query)
}

fn pick_from_html(html: &str, query: &str) -> Option<(String, String)> {
    let data = initial_data(html)?;
    let mut rows = Vec::new();
    collect_video_rows(&data, &mut rows);
    super::resolve::pick_best(&rows.join("\n"), query)
}

fn initial_data(html: &str) -> Option<Value> {
    const MARKERS: &[&str] = &[
        "var ytInitialData =",
        "window[\"ytInitialData\"] =",
        "ytInitialData =",
    ];
    for marker in MARKERS {
        let Some(start) = html.find(marker) else {
            continue;
        };
        let tail = &html[start + marker.len()..];
        let Some(open) = tail.find('{') else {
            continue;
        };
        let Some(json) = balanced_object(&tail[open..]) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str(json) {
            return Some(value);
        }
    }
    None
}

/// Return the first complete JSON object while respecting braces inside JSON
/// strings. YouTube appends JavaScript after the object, so parsing the rest of
/// the page as JSON would fail.
fn balanced_object(input: &str) -> Option<&str> {
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&input[..index + ch.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

fn collect_video_rows(value: &Value, rows: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(renderer) = map.get("videoRenderer")
                && let Some(row) = video_row(renderer)
            {
                rows.push(row);
            }
            for child in map.values() {
                collect_video_rows(child, rows);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_video_rows(child, rows);
            }
        }
        _ => {}
    }
}

fn video_row(renderer: &Value) -> Option<String> {
    let id = renderer.get("videoId")?.as_str()?.trim();
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let title = text(renderer.get("title")?)?;
    let channel = renderer
        .get("ownerText")
        .or_else(|| renderer.get("longBylineText"))
        .and_then(text)
        .unwrap_or_default();
    let views = renderer
        .get("viewCountText")
        .and_then(text)
        .map(|s| s.chars().filter(char::is_ascii_digit).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "0".to_owned());
    Some(format!(
        "{}\t{}\t{}\t{}",
        id,
        one_line(&title),
        one_line(&channel),
        views
    ))
}

fn text(value: &Value) -> Option<String> {
    if let Some(simple) = value.get("simpleText").and_then(Value::as_str) {
        return Some(simple.to_owned());
    }
    let joined = value
        .get("runs")?
        .as_array()?
        .iter()
        .filter_map(|run| run.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!joined.is_empty()).then_some(joined)
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_ranks_embedded_search_rows() {
        let html = r#"<script>var ytInitialData = {"contents":[
          {"videoRenderer":{"videoId":"studio_id","title":{"runs":[{"text":"Song (Official Video)"}]},"ownerText":{"runs":[{"text":"ArtistVEVO"}]},"viewCountText":{"simpleText":"1,200,000 views"}}},
          {"videoRenderer":{"videoId":"cover_id","title":{"runs":[{"text":"Song cover by Platinum Blues"}]},"ownerText":{"runs":[{"text":"Platinum Blues"}]},"viewCountText":{"simpleText":"15,000 views"}}}
        ]};</script>"#;
        assert_eq!(
            pick_from_html(html, "song cover Platinum Blues").unwrap().0,
            "cover_id"
        );
    }

    #[test]
    fn balanced_json_ignores_braces_inside_strings() {
        assert_eq!(
            balanced_object(r#"{"title":"a } b","ok":true}; tail"#),
            Some(r#"{"title":"a } b","ok":true}"#)
        );
    }

    #[test]
    fn rejects_non_video_ids() {
        let renderer = serde_json::json!({
            "videoId": "bad&id",
            "title": {"simpleText": "Song"}
        });
        assert!(video_row(&renderer).is_none());
    }

    #[tokio::test]
    #[ignore = "requires live YouTube access"]
    async fn resolves_public_search_live() {
        let (id, title) = resolve("Don't Matter cover by Platinum Blues")
            .await
            .expect("public YouTube search should return a playable video");
        assert!(!id.is_empty());
        assert!(!title.is_empty());
        println!("resolved {id}: {title}");
    }
}
