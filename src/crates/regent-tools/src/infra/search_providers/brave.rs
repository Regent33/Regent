//! Brave Search API — GET with an `X-Subscription-Token` header.
use super::{Method, SearchProvider, SearchRequest, SearchResult, map_results};
use serde_json::Value;

pub struct Brave;

impl SearchProvider for Brave {
    fn name(&self) -> &str {
        "brave"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("BRAVE_API_KEY")
    }
    fn build_request(&self, query: &str, api_key: Option<&str>, count: usize) -> SearchRequest {
        SearchRequest {
            method: Method::Get,
            url: "https://api.search.brave.com/res/v1/web/search".into(),
            query: vec![
                ("q".into(), query.into()),
                ("count".into(), count.to_string()),
            ],
            headers: vec![
                ("Accept".into(), "application/json".into()),
                (
                    "X-Subscription-Token".into(),
                    api_key.unwrap_or_default().into(),
                ),
            ],
            body: None,
        }
    }
    fn parse_response(&self, body: &[u8]) -> Result<Vec<SearchResult>, String> {
        let v: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let results = v
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(Value::as_array)
            .ok_or("brave: no web.results in response")?;
        Ok(map_results(results, "title", "url", "description"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_web_results() {
        let body = br#"{"web":{"results":[
            {"title":"Rust","url":"https://rust-lang.org","description":"A language"}]}}"#;
        let out = Brave.parse_response(body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].url, "https://rust-lang.org");
        assert_eq!(out[0].title, "Rust");
    }

    #[test]
    fn request_carries_token_header() {
        let req = Brave.build_request("cats", Some("k123"), 5);
        assert!(
            req.headers
                .iter()
                .any(|(h, v)| h == "X-Subscription-Token" && v == "k123")
        );
        assert!(req.query.iter().any(|(k, v)| k == "q" && v == "cats"));
    }
}
