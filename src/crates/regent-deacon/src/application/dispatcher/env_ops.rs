//! `env.*` — UI-driven read/write of API keys in `$REGENT_HOME/.env`. Values
//! are NEVER returned (list reports set + a `****last4` mask only); writes go
//! through the same owner-only upsert the `manage_keys` tool uses. The settable
//! surface is bounded (LLM provider keys + a key-suffix rule) with the runtime/
//! model-routing vars hard-blocked, so the UI can't inject PATH / REGENT_HOME /
//! model wiring (that belongs in `config.set`) through here.

use super::Dispatcher;
use super::config_ops::set_config_path;
use crate::domain::config::{DeaconConfig, MAX_KEY_SLOTS, ProviderKind};
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_tools::{
    MANAGED, env_var_status, extra_key_groups, key_group, remove_env_var, swap_env_vars,
    upsert_env_var,
};
use serde_json::{Value, json};

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
    ("NVIDIA_API_KEY", "NVIDIA (NIM)"),
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

/// If `name` is a numbered key variant (`<BASE>_2`, `<BASE>_3`, …) return its
/// base. Slot 1 is the unsuffixed base, so only `_2` and up count; `_1`/`_0` and
/// non-numeric tails (e.g. `_2X`) are not numbered variants and yield `None`.
fn numbered_base(name: &str) -> Option<&str> {
    let (base, suffix) = name.rsplit_once('_')?;
    let n: u32 = suffix.parse().ok()?;
    (2..=MAX_KEY_SLOTS as u32).contains(&n).then_some(base)
}

/// A name is settable if it's a plain UPPER_SNAKE identifier, not blocked, and
/// looks like a credential (a known LLM key or a key/token/secret/url suffix).
/// Numbered variants of a settable base (`OPENROUTER_API_KEY_2`) are settable
/// too — that's the multiple-keys-per-provider convention.
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
    // A numbered variant is settable iff its base is (and the base isn't blocked).
    let base = numbered_base(name).unwrap_or(name);
    if BLOCKED.contains(&base) {
        return false;
    }
    LLM_KEYS.iter().any(|(k, _)| *k == base)
        || [
            "_API_KEY", "_TOKEN", "_SECRET", "_KEY", "_URL", "_CX", "_ID", "_SID",
        ]
        .iter()
        .any(|suf| base.ends_with(suf))
}

/// A just-saved key is invisible in Settings → Model until a `providers:`
/// entry exists (the picker lists only `config.providers`) — so when the var
/// is the conventional key of a known provider kind and config has no provider
/// of that kind (nor any entry reading this var), return the `(dotted path,
/// value)` for the minimal entry to add. `None` = nothing to add: generic/
/// non-provider keys, kinds already configured, or a name already taken
/// (never overwrite a user's entry).
fn auto_provider(cfg: &DeaconConfig, saved: &str) -> Option<(String, Value)> {
    let base = numbered_base(saved).unwrap_or(saved);
    // Conventional vars are `<KIND>_API_KEY` with the kind's wire name — the
    // round-trip check below rejects lookalikes (TAVILY_…, REGENT_API_KEY).
    let kind_name = base.strip_suffix("_API_KEY")?.to_ascii_lowercase();
    let kind = ProviderKind::parse(&kind_name)?;
    if kind.key_env_var() != base
        || cfg.providers.contains_key(&kind_name)
        || cfg
            .providers
            .values()
            .any(|s| s.kind == kind || s.api_key_env == base)
    {
        return None;
    }
    Some((
        format!("providers.{kind_name}"),
        json!({ "kind": kind_name, "api_key_env": base }),
    ))
}

/// One `env.list` row: name/label, set-state, masked tail (never the value),
/// and the UI `group` ("llm" | "messaging" | "search" | "speech").
fn key_row(name: &str, label: &str, group: &str) -> Value {
    let (set, masked) = env_var_status(name);
    json!({ "name": name, "label": label, "set": set, "masked": masked, "group": group })
}

/// The full managed key set for `env.list`: the LLM provider keys (incl. the
/// generic REGENT_API_KEY fallback), then the messaging/search/speech keys from
/// the shared `MANAGED` table (its LLM entries are already covered by LLM_KEYS).
/// Numbered multi-key slots (`<BASE>_2`…) are listed only when actually set,
/// right after their base row.
fn env_key_rows() -> Vec<Value> {
    let mut triples: Vec<(&str, String, &str)> = LLM_KEYS
        .iter()
        .map(|(name, label)| (*name, (*label).to_owned(), "llm"))
        .collect();
    triples.extend(
        MANAGED
            .iter()
            .filter(|(name, _)| key_group(name) != "llm")
            .map(|(name, label)| (*name, (*label).to_owned(), key_group(name))),
    );
    // Keys serving several generation products (Kling/Higgsfield do video AND
    // photo) get one extra row per additional group — same env var either way.
    let extras: Vec<(&str, String, &str)> = triples
        .iter()
        .flat_map(|(name, label, _)| {
            extra_key_groups(name)
                .iter()
                .map(|g| (*name, label.clone(), *g))
                .collect::<Vec<_>>()
        })
        .collect();
    triples.extend(extras);
    let mut rows = Vec::new();
    for (name, label, group) in triples {
        rows.push(key_row(name, &label, group));
        for slot in 2..=MAX_KEY_SLOTS {
            let var = format!("{name}_{slot}");
            if env_var_status(&var).0 {
                rows.push(key_row(&var, &format!("{label} ({slot})"), group));
            }
        }
    }
    rows
}

impl Dispatcher {
    /// `env.list` — the managed keys grouped for the UI, each with set-state +
    /// masked tail (no values).
    pub(super) fn env_list(&self, req: RpcRequest) {
        self.send(ok_response(req.id, json!({ "keys": env_key_rows() })));
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
                // upsert_env_var hot-applied the process env; the reload also
                // rebuilds provider routing so cached providers holding a
                // stale key are dropped.
                self.reapply_config();
                // New key for an unconfigured provider kind → add its minimal
                // `providers:` entry through the validated config path, so the
                // provider shows up in Settings → Model right away. Best-effort:
                // the key save already succeeded; a failure here only warns.
                let mut note = "saved to .env and applied — takes effect from your next message (open sessions re-route too)".to_owned();
                if let (Some(cfg), Ok(home)) =
                    (self.config_snapshot(), std::env::var("REGENT_HOME"))
                    && let Some((path, value)) = auto_provider(&cfg, &name)
                {
                    match set_config_path(std::path::Path::new(&home), &path, &value) {
                        Ok((_, parsed)) => {
                            let provider = path.trim_start_matches("providers.").to_owned();
                            self.apply_config(parsed);
                            note = format!(
                                "saved to .env and applied — provider '{provider}' added to config.yaml; pick its model in Settings → Model"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(%error, key = %name, "provider auto-add skipped");
                        }
                    }
                }
                let (_, masked) = env_var_status(&name);
                self.send(ok_response(
                    req.id,
                    json!({ "name": name, "masked": masked, "note": note }),
                ));
            }
            Err(e) => self.send(err_response(req.id, -32000, e)),
        }
    }

    /// `env.activate {name, slot}` — make numbered slot N the ACTIVE key for a
    /// base var by swapping values (the runtime resolves the base first). Both
    /// keys stay stored; only which one is "key 1" changes.
    pub(super) fn env_activate(&self, req: RpcRequest) {
        let Some(name) = req.params.get("name").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "missing name"));
            return;
        };
        let name = name.trim().to_uppercase();
        let slot = req.params.get("slot").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        if !is_settable(&name) || numbered_base(&name).is_some() {
            self.send(err_response(
                req.id,
                -32602,
                format!("{name} is not a base key managed here"),
            ));
            return;
        }
        if !(2..=MAX_KEY_SLOTS).contains(&slot) {
            self.send(err_response(
                req.id,
                -32602,
                format!("slot must be 2..={MAX_KEY_SLOTS}"),
            ));
            return;
        }
        match swap_env_vars(&name, &format!("{name}_{slot}")) {
            Ok(()) => {
                self.reapply_config();
                let (_, masked) = env_var_status(&name);
                self.send(ok_response(
                    req.id,
                    json!({ "name": name, "masked": masked, "note": "swapped and applied — takes effect from your next message (open sessions re-route too)" }),
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
            Ok(removed) => {
                self.reapply_config();
                self.send(ok_response(
                    req.id,
                    json!({ "name": name, "removed": removed }),
                ));
            }
            Err(e) => self.send(err_response(req.id, -32000, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{auto_provider, env_key_rows, is_settable};
    use crate::domain::config::{DeaconConfig, ProviderKind, ProviderSpec};

    #[test]
    fn a_new_provider_key_yields_a_config_entry_that_survives_the_write_gate() {
        let cfg = DeaconConfig::default();
        // The reported bug: NVIDIA_API_KEY saved, no `nvidia` provider → the
        // Model page (which lists only config.providers) never shows it.
        let (path, value) = auto_provider(&cfg, "NVIDIA_API_KEY").expect("adds nvidia");
        assert_eq!(path, "providers.nvidia");
        // A numbered slot behaves like its base var.
        assert!(auto_provider(&cfg, "GROQ_API_KEY_2").is_some());
        // The generated value must pass the same whole-file validation
        // config.set applies — otherwise the auto-add silently no-ops.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.yaml"), "_config_version: 1\n").unwrap();
        let (_, parsed) =
            super::super::config_ops::set_config_path(dir.path(), &path, &value).unwrap();
        let spec = parsed.providers.get("nvidia").expect("entry persisted");
        assert_eq!(spec.kind, ProviderKind::Nvidia);
        assert_eq!(spec.api_key_env, "NVIDIA_API_KEY");
    }

    #[test]
    fn auto_provider_never_duplicates_or_clobbers() {
        let mut cfg = DeaconConfig::default();
        // Non-provider / generic keys map to nothing.
        assert!(auto_provider(&cfg, "REGENT_API_KEY").is_none());
        assert!(auto_provider(&cfg, "TAVILY_API_KEY").is_none());
        assert!(auto_provider(&cfg, "SLACK_BOT_TOKEN").is_none());
        // A same-kind entry under ANY name blocks the add (the real config
        // shape: `ollama-cloud` of kind ollama reading OLLAMA_API_KEY).
        cfg.providers.insert(
            "ollama-cloud".to_owned(),
            ProviderSpec {
                kind: ProviderKind::Ollama,
                api_key_env: "OLLAMA_API_KEY".to_owned(),
                ..ProviderSpec::default()
            },
        );
        assert!(auto_provider(&cfg, "OLLAMA_API_KEY").is_none());
        // An entry already reading the var blocks it even under another kind…
        cfg.providers.insert(
            "my-gateway".to_owned(),
            ProviderSpec {
                kind: ProviderKind::Openai,
                api_key_env: "GROQ_API_KEY".to_owned(),
                ..ProviderSpec::default()
            },
        );
        assert!(auto_provider(&cfg, "GROQ_API_KEY").is_none());
        // …and a taken name is never overwritten.
        cfg.providers.insert(
            "mistral".to_owned(),
            ProviderSpec {
                kind: ProviderKind::Openai,
                ..ProviderSpec::default()
            },
        );
        assert!(auto_provider(&cfg, "MISTRAL_API_KEY").is_none());
    }

    #[test]
    fn env_list_surfaces_a_messaging_key_grouped_and_masked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "REGENT_TELEGRAM_TOKEN=bot-secret-9876\nOPENROUTER_API_KEY_2=second-key-4321\n",
        )
        .unwrap();
        // SAFETY: single-threaded test; env_var_status reads REGENT_HOME/.env.
        unsafe { std::env::set_var("REGENT_HOME", dir.path()) };

        let rows = env_key_rows();
        let tg = rows
            .iter()
            .find(|r| r["name"] == "REGENT_TELEGRAM_TOKEN")
            .expect("telegram token is in the managed set");
        assert_eq!(tg["group"], "messaging");
        assert_eq!(tg["set"], true);
        assert_eq!(tg["masked"], "****9876");
        // The raw value must never be returned.
        assert!(!tg.to_string().contains("bot-secret-9876"));
        // LLM provider keys stay in the "llm" group (older/flat clients ok).
        let anthropic = rows
            .iter()
            .find(|r| r["name"] == "ANTHROPIC_API_KEY")
            .expect("anthropic key present");
        assert_eq!(anthropic["group"], "llm");
        // A SET numbered slot shows up beside its base with a slot label…
        let second = rows
            .iter()
            .find(|r| r["name"] == "OPENROUTER_API_KEY_2")
            .expect("set _2 slot is listed");
        assert_eq!(second["group"], "llm");
        assert_eq!(second["label"], "OpenRouter (2)");
        assert_eq!(second["masked"], "****4321");
        // …but unset slots are never listed.
        assert!(!rows.iter().any(|r| r["name"] == "ANTHROPIC_API_KEY_2"));
    }

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
        // Numbered multi-key slots: settable iff the base is.
        assert!(is_settable("OPENROUTER_API_KEY_2"));
        assert!(is_settable("SLACK_BOT_TOKEN_3"));
        assert!(!is_settable("OPENROUTER_API_KEY_2X"));
        assert!(!is_settable("REGENT_HOME_2"));
        // Not a credential shape.
        assert!(!is_settable("RANDOM_FLAG"));
        assert!(!is_settable("lowercase_key"));
        assert!(!is_settable(""));
    }
}
