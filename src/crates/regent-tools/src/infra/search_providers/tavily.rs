//! Tavily — AI-oriented search; POST with the key in the JSON body.
use super::{Method, SearchProvider, SearchRequest, SearchResult, map_results};
use serde_json::{Value, json};

pub struct Tavily;

impl SearchProvider for Tavily {
    fn name(&self) -> &str {
        "tavily"
    }
    fn key_env(&self) -> Option<&'static str> {
        Some("TAVILY_API_KEY")
    }
    fn build_request(&self, query: &str, api_key: Option<&str>, count: usize) -> SearchRequest {
        SearchRequest {
            method: Method::Post,
            url: "https://api.tavily.com/search".into(),
            query: vec![],
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Some(json!({
                "api_key": api_key.unwrap_or_default(),
                "query": query,
                "max_results": count,
            })),
        }
    }
    fn parse_response(&self, body: &[u8]) -> Result<Vec<SearchResult>, String> {
        let v: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let results = v
            .get("results")
            .and_then(Value::as_array)
            .ok_or("tavily: no results")?;
        Ok(map_results(results, "title", "url", "content"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_results() {
        let body = br#"{"results":[{"title":"T","url":"https://x.io","content":"snip"}]}"#;
        let out = Tavily.parse_response(body).unwrap();
        assert_eq!(out[0].snippet, "snip");
    }

    #[test]
    fn key_goes_in_body() {
        let req = Tavily.build_request("q", Some("sk"), 3);
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.body.unwrap()["api_key"], "sk");
    }
}
