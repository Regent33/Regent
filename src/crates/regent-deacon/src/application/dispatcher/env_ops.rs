//! `env.*` — UI-driven read/write of API keys in `$REGENT_HOME/.env`. Values
//! are NEVER returned (list reports set + a `****last4` mask only); writes go
//! through the same owner-only upsert the `manage_keys` tool uses. The settable
//! surface is bounded (LLM provider keys + a key-suffix rule) with the runtime/
//! model-routing vars hard-blocked, so the UI can't inject PATH / REGENT_HOME /
//! model wiring (that belongs in `config.set`) through here.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_tools::{env_var_status, remove_env_var, upsert_env_var};
use serde_json::json;

/// The LLM provider key vars the API Keys page surfaces (var name, label).
/// REGENT_API_KEY is the generic default the deacon falls back to (§ provider
/// key resolution). The user setting it here is legitimate — unlike the agent
/// `manage_keys` tool, which protects it from self-clobbering.
const LLM_KEYS: &[(&str, &str)] = &[
    ("REGENT_API_KEY", "Default (generic fallback)"),
    ("ANTHROPIC_API_KEY", "Anthropic"),
    ("OPENAI_API_KEY", "OpenAI"),
    ("OPENROUTER_API_KEY", "OpenRouter"),
    ("GROQ_API_KEY", "Groq"),
    ("DEEPSEEK_API_KEY", "DeepSeek"),
    ("TOGETHER_API_KEY", "Together"),
    ("OLLAMA_API_KEY", "Ollama"),
    ("MISTRAL_API_KEY", "Mistral"),
    ("XAI_API_KEY", "xAI (Grok)"),
    ("GEMINI_API_KEY", "Google Gemini"),
    ("MOONSHOT_API_KEY", "Moonshot (Kimi)"),
    ("ZHIPU_API_KEY", "Zhipu (GLM/Z.AI)"),
    ("DASHSCOPE_API_KEY", "DashScope (Qwen)"),
    ("FIREWORKS_API_KEY", "Fireworks"),
    ("CEREBRAS_API_KEY", "Cerebras"),
    ("PERPLEXITY_API_KEY", "Perplexity"),
    ("MINIMAX_API_KEY", "MiniMax"),
];

/// Never writable through the UI: process runtime + model-routing (use
/// `config.set` for provider/model/base_url so the validated schema applies).
const BLOCKED: &[&str] = &[
    "REGENT_HOME",
    "REGENT_NOW",
    "REGENT_MODEL",
    "REGENT_PROVIDER",
    "REGENT_BASE_URL",
    "PATH",
];

/// A name is settable if it's a plain UPPER_SNAKE identifier, not blocked, and
/// looks like a credential (a known LLM key or a key/token/secret/url suffix).
fn is_settable(name: &str) -> bool {
    if name.is_empty()
        || BLOCKED.contains(&name)
        || !name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        || !name.as_bytes()[0].is_ascii_uppercase()
    {
        return false;
    }
    LLM_KEYS.iter().any(|(k, _)| *k == name)
        || [
            "_API_KEY", "_TOKEN", "_SECRET", "_KEY", "_URL", "_CX", "_ID", "_SID",
        ]
        .iter()
        .any(|suf| name.ends_with(suf))
}

impl Dispatcher {
    /// `env.list` — the LLM provider keys with set-state + masked tail (no values).
    pub(super) fn env_list(&self, req: RpcRequest) {
        let items: Vec<_> = LLM_KEYS
            .iter()
            .map(|(name, label)| {
                let (set, masked) = env_var_status(name);
                json!({ "name": name, "label": label, "set": set, "masked": masked })
            })
            .collect();
        self.send(ok_response(req.id, json!({ "keys": items })));
    }

    /// `env.set {name, value}` — persist a key to `.env` (masked in the reply).
    pub(super) fn env_set(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        let name = name.trim().to_uppercase();
        if !is_settable(&name) {
            self.send(err_response(
                req.id,
                -32602,
                format!("{name} is not a settable key here"),
            ));
            return;
        }
        let value = req
            .params
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if value.is_empty() {
            self.send(err_response(req.id, -32602, "missing or empty value"));
            return;
        }
        match upsert_env_var(&name, value) {
            Ok(()) => {
                let (_, masked) = env_var_status(&name);
                self.send(ok_response(
                    req.id,
                    json!({ "name": name, "masked": masked, "note": "saved to .env; applies on the next deacon start" }),
                ));
            }
            Err(e) => self.send(err_response(req.id, -32000, e)),
        }
    }

    /// `env.unset {name}` — remove a key from `.env`.
    pub(super) fn env_unset(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        let name = name.trim().to_uppercase();
        if !is_settable(&name) {
            self.send(err_response(
                req.id,
                -32602,
                format!("{name} is not managed here"),
            ));
            return;
        }
        match remove_env_var(&name) {
            Ok(removed) => self.send(ok_response(
                req.id,
                json!({ "name": name, "removed": removed }),
            )),
            Err(e) => self.send(err_response(req.id, -32000, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_settable;

    #[test]
    fn settable_covers_llm_and_credential_suffixes_but_blocks_runtime() {
        assert!(is_settable("OLLAMA_API_KEY"));
        assert!(is_settable("OPENROUTER_API_KEY"));
        assert!(is_settable("REGENT_API_KEY")); // the user's own model key
        assert!(is_settable("TAVILY_API_KEY"));
        assert!(is_settable("SLACK_BOT_TOKEN"));
        // Blocked runtime / model-routing (use config.set for those).
        assert!(!is_settable("REGENT_HOME"));
        assert!(!is_settable("PATH"));
        assert!(!is_settable("REGENT_MODEL"));
        // Not a credential shape.
        assert!(!is_settable("RANDOM_FLAG"));
        assert!(!is_settable("lowercase_key"));
        assert!(!is_settable(""));
    }
}
