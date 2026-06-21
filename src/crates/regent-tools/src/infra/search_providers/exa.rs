//! Exa — neural search; POST with an `x-api-key` header.
use super::{Method, SearchProvider, SearchRequest, SearchResult, map_results};
use serde_json::{Value, json};

pub struct Exa;

impl SearchProvider for Exa {
    fn name(&self) -> &str {
        "exa"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("EXA_API_KEY")
    }
    fn build_request(&self, query: &str, api_key: Option<&str>, count: usize) -> SearchRequest {
        SearchRequest {
            method: Method::Post,
            url: "https://api.exa.ai/search".into(),
            query: vec![],
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("x-api-key".into(), api_key.unwrap_or_default().into()),
            ],
            body: Some(json!({ "query": query, "numResults": count })),
        }
    }
    fn parse_response(&self, body: &[u8]) -> Result<Vec<SearchResult>, String> {
        let v: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let results = v
            .get("results")
            .and_then(Value::as_array)
            .ok_or("exa: no results")?;
        // Exa returns title/url, with optional `text` used as the snippet.
        Ok(map_results(results, "title", "url", "text"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_results_and_sets_key_header() {
        let body = br#"{"results":[{"title":"T","url":"https://x.io","text":"t"}]}"#;
        assert_eq!(Exa.parse_response(body).unwrap()[0].snippet, "t");
        let req = Exa.build_request("q", Some("ek"), 4);
        assert!(
            req.headers
                .iter()
                .any(|(h, v)| h == "x-api-key" && v == "ek")
        );
    }
}
