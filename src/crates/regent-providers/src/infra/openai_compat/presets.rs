//! Named OpenAI-compatible endpoint presets.

use super::OpenAiCompatChatConfig;

impl OpenAiCompatChatConfig {
    /// OpenAI (`api.openai.com`).
    #[must_use]
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.openai.com", api_key, model)
    }

    /// OpenRouter (`openrouter.ai`) — hundreds of models behind one key.
    #[must_use]
    pub fn openrouter(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://openrouter.ai/api", api_key, model)
    }

    /// Groq (`api.groq.com`) — fast hosted open models.
    #[must_use]
    pub fn groq(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.groq.com/openai", api_key, model)
    }

    /// DeepSeek (`api.deepseek.com`).
    #[must_use]
    pub fn deepseek(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.deepseek.com", api_key, model)
    }

    /// Together AI (`api.together.xyz`).
    #[must_use]
    pub fn together(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new("https://api.together.xyz", api_key, model)
    }

    /// Local Ollama (`localhost:11434`, no key) via OpenAI compatibility.
    #[must_use]
    pub fn ollama(model: impl Into<String>) -> Self {
        Self::new("http://localhost:11434", "", model)
    }
}
