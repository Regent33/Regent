//! Google Programmable Search (Custom Search JSON API) — needs an API key *and*
//! a search-engine id (`cx`), read from `GOOGLE_CSE_CX`.
use super::{Method, SearchProvider, SearchRequest, SearchResult, map_results};
use serde_json::Value;

pub struct GoogleCse;

impl SearchProvider for GoogleCse {
    fn name(&self) -> &str {
        "google_cse"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("GOOGLE_CSE_API_KEY")
    }
    fn build_request(&self, query: &str, api_key: Option<&str>, count: usize) -> SearchRequest {
        // `cx` is secondary config, not the secret key; read it here.
        let cx = std::env::var("GOOGLE_CSE_CX").unwrap_or_default();
        SearchRequest {
            method: Method::Get,
            url: "https://www.googleapis.com/customsearch/v1".into(),
            query: vec![
                ("key".into(), api_key.unwrap_or_default().into()),
                ("cx".into(), cx),
                ("q".into(), query.into()),
                ("num".into(), count.min(10).to_string()),
            ],
            headers: vec![],
            body: None,
        }
    }
    fn parse_response(&self, body: &[u8]) -> Result<Vec<SearchResult>, String> {
        let v: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let items = v
            .get("items")
            .and_then(Value::as_array)
            .ok_or("google_cse: no items")?;
        Ok(map_results(items, "title", "link", "snippet"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_items() {
        let body = br#"{"items":[{"title":"T","link":"https://x.io","snippet":"s"}]}"#;
        assert_eq!(GoogleCse.parse_response(body).unwrap()[0].title, "T");
    }
}
