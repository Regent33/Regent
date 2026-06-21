//! DuckDuckGo Instant Answer API — keyless, so it's the out-of-the-box default.
//! Note: this is the Instant Answer endpoint (topics/abstract), not full web
//! results — fine as a no-key fallback; configure another provider for depth.
use super::{Method, SearchProvider, SearchRequest, SearchResult};
use serde_json::Value;

pub struct DuckDuckGo;

impl SearchProvider for DuckDuckGo {
    fn name(&self) -> &str {
        "duckduckgo"
    }
    fn key_env(&self) -> Option<&'static str> {
        None
    }
    fn build_request(&self, query: &str, _api_key: Option<&str>, _count: usize) -> SearchRequest {
        SearchRequest {
            method: Method::Get,
            url: "https://api.duckduckgo.com/".into(),
            query: vec![
                ("q".into(), query.into()),
                ("format".into(), "json".into()),
                ("no_html".into(), "1".into()),
                ("no_redirect".into(), "1".into()),
            ],
            headers: vec![],
            body: None,
        }
    }
    fn parse_response(&self, body: &[u8]) -> Result<Vec<SearchResult>, String> {
        let v: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        // The primary abstract, when present.
        if let Some(text) = v
            .get("AbstractText")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            out.push(SearchResult {
                title: v
                    .get("Heading")
                    .and_then(Value::as_str)
                    .unwrap_or("Result")
                    .to_owned(),
                url: v
                    .get("AbstractURL")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                snippet: text.to_owned(),
            });
        }
        // Related topics (some are nested groups under "Topics").
        collect_topics(v.get("RelatedTopics"), &mut out);
        if out.is_empty() {
            return Err("duckduckgo: no instant-answer results (try a keyed provider)".into());
        }
        Ok(out)
    }
}

fn collect_topics(node: Option<&Value>, out: &mut Vec<SearchResult>) {
    let Some(arr) = node.and_then(Value::as_array) else {
        return;
    };
    for t in arr {
        if let Some(sub) = t.get("Topics") {
            collect_topics(Some(sub), out);
            continue;
        }
        let (Some(text), Some(url)) = (
            t.get("Text").and_then(Value::as_str),
            t.get("FirstURL").and_then(Value::as_str),
        ) else {
            continue;
        };
        let title = text.split(" - ").next().unwrap_or(text);
        out.push(SearchResult {
            title: title.to_owned(),
            url: url.to_owned(),
            snippet: text.to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_abstract_and_related_topics() {
        let body = br#"{"Heading":"Rust","AbstractText":"A language","AbstractURL":"https://r.org",
            "RelatedTopics":[{"Text":"Rust book - the guide","FirstURL":"https://doc.r.org"}]}"#;
        let out = DuckDuckGo.parse_response(body).unwrap();
        assert_eq!(out[0].url, "https://r.org");
        assert_eq!(out[1].title, "Rust book");
    }

    #[test]
    fn empty_is_an_error() {
        assert!(
            DuckDuckGo
                .parse_response(br#"{"RelatedTopics":[]}"#)
                .is_err()
        );
    }
}
