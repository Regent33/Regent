//! `dictionary` — English (or other-language) word definitions from the free,
//! keyless dictionaryapi.dev. Groups definitions by part of speech, keeps up
//! to ~3 senses each, and surfaces phonetics/synonyms when present.

use super::http;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json, tool_result_json};
use serde_json::{Value, json};

const BASE_URL: &str = "https://api.dictionaryapi.dev";
const MAX_SENSES_PER_MEANING: usize = 3;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "dictionary".into(),
        description: "Look up a word's definitions, grouped by part of speech, with phonetics \
                      and synonyms when available. No API key needed."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "word": {"type": "string", "description": "The word to look up."},
                "language": {"type": "string", "description": "Language code (default 'en')."}
            },
            "required": ["word"]
        }),
        toolset: "everyday".into(),
    }
}

pub struct DictionaryTool;

#[async_trait]
impl ToolExecutor for DictionaryTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(word) = args
            .get("word")
            .and_then(Value::as_str)
            .filter(|w| !w.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: word"));
        };
        let language = args
            .get("language")
            .and_then(Value::as_str)
            .filter(|l| !l.trim().is_empty())
            .unwrap_or("en");

        match lookup(language, word).await {
            Ok(data) => Ok(tool_result_json(data)),
            Err(e) => Ok(tool_error_json(format!("dictionary: {e}"))),
        }
    }
}

/// Build the dictionaryapi.dev lookup URL (pure — no I/O; segments are
/// percent-encoded by `Url::path_segments_mut`).
pub fn build_url(language: &str, word: &str) -> String {
    let mut url = reqwest::Url::parse(BASE_URL).expect("static url");
    url.path_segments_mut()
        .expect("base url has a host")
        .push("api")
        .push("v2")
        .push("entries")
        .push(language)
        .push(word);
    url.to_string()
}

async fn lookup(language: &str, word: &str) -> Result<Value, String> {
    let url = build_url(language, word);
    let (status, bytes) = http::fetch("dictionaryapi.dev", &url).await?;
    if status == 404 {
        return Err(format!("no entry found for '{word}'"));
    }
    if !(200..300).contains(&status) {
        let snippet: String = String::from_utf8_lossy(&bytes).chars().take(300).collect();
        return Err(format!("dictionaryapi.dev HTTP {status}: {snippet}"));
    }
    parse_dictionary_response(&bytes, word)
}

/// Parse a dictionaryapi.dev response body into `{word, phonetic, meanings}`
/// (pure). A non-array body (the API's 404 shape) or an empty array both
/// mean "no entry found".
pub fn parse_dictionary_response(body: &[u8], word: &str) -> Result<Value, String> {
    let v: Value =
        serde_json::from_slice(body).map_err(|e| format!("bad dictionary response: {e}"))?;
    let entries = v
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| format!("no entry found for '{word}'"))?;
    let first = &entries[0];

    let phonetics: Vec<&str> = first
        .get("phonetics")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let phonetic = first
        .get("phonetic")
        .and_then(Value::as_str)
        .or_else(|| phonetics.first().copied());

    let mut meanings_out = Vec::new();
    for entry in entries {
        let Some(meanings) = entry.get("meanings").and_then(Value::as_array) else {
            continue;
        };
        for m in meanings {
            meanings_out.push(meaning_json(m));
        }
    }

    Ok(json!({
        "word": first.get("word").and_then(Value::as_str).unwrap_or(word),
        "phonetic": phonetic,
        "meanings": meanings_out,
    }))
}

fn meaning_json(m: &Value) -> Value {
    let part_of_speech = m
        .get("partOfSpeech")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let definitions: Vec<Value> = m
        .get("definitions")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .take(MAX_SENSES_PER_MEANING)
                .map(|d| {
                    json!({
                        "definition": d.get("definition").and_then(Value::as_str).unwrap_or(""),
                        "example": d.get("example").and_then(Value::as_str),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let synonyms: Vec<&str> = m
        .get("synonyms")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    json!({
        "part_of_speech": part_of_speech,
        "definitions": definitions,
        "synonyms": synonyms,
    })
}

#[cfg(test)]
#[path = "tests/dictionary.rs"]
mod tests;
