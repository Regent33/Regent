//! SerpAPI — Google results via a GET with the key as a query param.
use super::{Method, SearchProvider, SearchRequest, SearchResult, map_results};
use serde_json::Value;

pub struct SerpApi;

impl SearchProvider for SerpApi {
    fn name(&self) -> &str {
        "serpapi"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("SERPAPI_API_KEY")
    }
    fn build_request(&self, query: &str, api_key: Option<&str>, count: usize) -> SearchRequest {
        SearchRequest {
            method: Method::Get,
            url: "https://serpapi.com/search.json".into(),
            query: vec![
                ("engine".into(), "google".into()),
                ("q".into(), query.into()),
                ("num".into(), count.to_string()),
                ("api_key".into(), api_key.unwrap_or_default().into()),
            ],
            headers: vec![],
            body: None,
        }
    }
    fn parse_response(&self, body: &[u8]) -> Result<Vec<SearchResult>, String> {
        let v: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let results = v
            .get("organic_results")
            .and_then(Value::as_array)
            .ok_or("serpapi: no organic_results")?;
        Ok(map_results(results, "title", "link", "snippet"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_organic_results() {
        let body = br#"{"organic_results":[{"title":"T","link":"https://x.io","snippet":"s"}]}"#;
        let out = SerpApi.parse_response(body).unwrap();
        assert_eq!(out[0].url, "https://x.io");
    }
}
