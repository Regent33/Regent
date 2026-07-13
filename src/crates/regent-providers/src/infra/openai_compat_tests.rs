//! Unit tests for `openai_compat` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn presets_target_the_expected_base_urls() {
    assert_eq!(
        OpenAiCompatChatConfig::openai("k", "m").base_url,
        "https://api.openai.com"
    );
    assert_eq!(
        OpenAiCompatChatConfig::openrouter("k", "m").base_url,
        "https://openrouter.ai/api"
    );
    assert_eq!(
        OpenAiCompatChatConfig::groq("k", "m").base_url,
        "https://api.groq.com/openai"
    );
    assert_eq!(
        OpenAiCompatChatConfig::deepseek("k", "m").base_url,
        "https://api.deepseek.com"
    );
    assert_eq!(
        OpenAiCompatChatConfig::together("k", "m").base_url,
        "https://api.together.xyz"
    );
}

#[test]
fn ollama_is_local_and_keyless() {
    let cfg = OpenAiCompatChatConfig::ollama("llama3");
    assert_eq!(cfg.base_url, "http://localhost:11434");
    assert_eq!(cfg.api_key, "");
    assert_eq!(cfg.api_path, "/v1/chat/completions");
}
