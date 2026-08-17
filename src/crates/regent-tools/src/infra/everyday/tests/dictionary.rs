//! Unit tests for `dictionary` — URL construction and response parsing
//! against canned JSON. No network involved.

use super::*;

const SAMPLE_ENTRY: &str = r#"[
    {
        "word": "hello",
        "phonetic": "həˈloʊ",
        "phonetics": [{"text": "həˈloʊ", "audio": "https://example.com/hello.mp3"}],
        "meanings": [
            {
                "partOfSpeech": "exclamation",
                "definitions": [
                    {"definition": "used as a greeting.", "example": "hello there!"},
                    {"definition": "an expression of surprise."}
                ],
                "synonyms": ["hi", "hey"]
            },
            {
                "partOfSpeech": "noun",
                "definitions": [
                    {"definition": "an utterance of 'hello'."},
                    {"definition": "second sense."},
                    {"definition": "third sense."},
                    {"definition": "fourth sense, dropped by the 3-cap."}
                ],
                "synonyms": []
            }
        ]
    }
]"#;

const NOT_FOUND_BODY: &str = r#"{
    "title": "No Definitions Found",
    "message": "Sorry pal, we couldn't find definitions for the word you were looking for.",
    "resolution": "You can try the search again at a later time or head to the web instead."
}"#;

#[test]
fn url_encodes_language_and_word_as_path_segments() {
    let url = build_url("en", "hello world");
    assert_eq!(
        url,
        "https://api.dictionaryapi.dev/api/v2/entries/en/hello%20world"
    );
}

#[test]
fn parses_word_phonetic_and_grouped_meanings() {
    let v = parse_dictionary_response(SAMPLE_ENTRY.as_bytes(), "hello").unwrap();
    assert_eq!(v["word"], "hello");
    assert!(v["phonetic"].as_str().unwrap().contains("lo"));
    let meanings = v["meanings"].as_array().unwrap();
    assert_eq!(meanings.len(), 2);
    assert_eq!(meanings[0]["part_of_speech"], "exclamation");
    let defs0 = meanings[0]["definitions"].as_array().unwrap();
    assert_eq!(defs0.len(), 2);
    assert_eq!(defs0[0]["definition"], "used as a greeting.");
    assert_eq!(defs0[0]["example"], "hello there!");
    assert_eq!(meanings[0]["synonyms"], json!(["hi", "hey"]));

    // Fourth sense of the noun meaning is dropped by the 3-cap.
    let defs1 = meanings[1]["definitions"].as_array().unwrap();
    assert_eq!(defs1.len(), 3);
}

#[test]
fn falls_back_to_a_phonetics_entry_when_top_level_phonetic_is_absent() {
    let body = r#"[{"word":"x","phonetics":[{"text":"/eks/"}],"meanings":[]}]"#;
    let v = parse_dictionary_response(body.as_bytes(), "x").unwrap();
    assert_eq!(v["phonetic"], "/eks/");
}

#[test]
fn not_found_object_body_is_a_clear_error() {
    let err = parse_dictionary_response(NOT_FOUND_BODY.as_bytes(), "zzzxyz").unwrap_err();
    assert!(err.contains("no entry found for 'zzzxyz'"), "{err}");
}

#[test]
fn empty_array_body_is_a_clear_error() {
    let err = parse_dictionary_response(b"[]", "zzzxyz").unwrap_err();
    assert!(err.contains("no entry found for 'zzzxyz'"), "{err}");
}

#[test]
fn malformed_json_is_a_clear_error() {
    let err = parse_dictionary_response(b"not json", "x").unwrap_err();
    assert!(err.contains("bad dictionary response"), "{err}");
}

#[tokio::test]
async fn missing_word_arg_is_a_clear_tool_error() {
    let ctx = ToolContext::new(
        std::path::PathBuf::from("."),
        std::sync::Arc::new(crate::domain::contracts::DenyAll),
    );
    let out = DictionaryTool.execute(json!({}), &ctx).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("missing required parameter: word"),
        "{v}"
    );
}
