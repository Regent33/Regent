//! Spawning the agent deacon for voice: env contract ported from
//! web_call.py's `_ensure_rpc` — the caller drives by voice and can't tap
//! "approve", so tool actions are auto-approved unless opted out, the session
//! answers in spoken style (`REGENT_VOICE=1`), and computer-use is on by
//! default so "look at my screen / open this site" works.

use crate::infra::deacon::{DeaconRpc, find_deacon};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;

pub enum AgentStatus {
    Ready(Arc<DeaconRpc>),
    /// Why the agent brain is off (logged once; /health shows it).
    Unavailable(String),
}

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"),
        Err(_) => default,
    }
}

pub(crate) fn regent_home() -> PathBuf {
    if let Ok(h) = std::env::var("REGENT_HOME") {
        return PathBuf::from(h);
    }
    let user = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(user).join(".regent")
}

/// Backfill the agent's env from `$REGENT_HOME/.env` so a manually started
/// server still gets provider credentials. Model selection is deliberately not
/// frozen into the child env: [`call_model_from`] rereads the active chat model
/// before every turn. The real environment always wins.
fn brain_env_from(home: &Path) -> HashMap<String, String> {
    let mut extra = HashMap::new();
    extra.insert("REGENT_HOME".into(), home.to_string_lossy().into_owned());
    if let Ok(dotenv) = std::fs::read_to_string(home.join(".env")) {
        for line in dotenv.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim().trim_matches('"'));
            if !line.starts_with('#') && !key.is_empty() && !value.is_empty() {
                extra.insert(key.to_owned(), value.to_owned());
            }
        }
    }
    // Explicit process env wins over the dotenv backfill.
    extra.retain(|key, _| std::env::var(key).is_err());
    extra
}

/// The model Butler should use for its next turn. An explicit voice-only model
/// wins; otherwise follow the same persisted provider/model pair as main chat.
/// Reading the file per turn lets a reused server follow hot swaps immediately.
pub(crate) fn call_model_from(home: &Path) -> Option<String> {
    fn text(value: Option<&serde_yaml::Value>) -> Option<&str> {
        value
            .and_then(serde_yaml::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
    let raw = std::fs::read_to_string(home.join("config.yaml")).ok()?;
    let doc = serde_yaml::from_str::<serde_yaml::Value>(&raw).ok()?;
    if let Some(fast) = text(
        doc.get("speech")
            .and_then(|speech| speech.get("call"))
            .and_then(|call| call.get("fast_model")),
    ) {
        return Some(fast.to_owned());
    }
    if let Some(primary) = doc
        .get("agents_defaults")
        .and_then(|agents| agents.get("primary"))
    {
        let provider = text(primary.get("provider"));
        let model = text(primary.get("model"));
        if let (Some(provider), Some(model)) = (provider, model) {
            return Some(format!("{provider}/{model}"));
        }
    }
    text(doc.get("model").and_then(|model| model.get("default"))).map(ToOwned::to_owned)
}

fn brain_env() -> HashMap<String, String> {
    brain_env_from(&regent_home())
}

/// Spawn the deacon (stdio JSON-RPC) and open a session. The child dies with
/// this process (`kill_on_drop`).
pub async fn spawn_agent() -> AgentStatus {
    let off = |reason: &str| AgentStatus::Unavailable(reason.to_owned());
    if !env_flag("REGENT_VOICE_AGENT", true) {
        return off("REGENT_VOICE_AGENT disabled");
    }
    let Some(deacon) = find_deacon() else {
        return off("regent-deacon binary not found");
    };
    let extra = brain_env();
    let mut cmd = Command::new(&deacon);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .envs(&extra)
        // The spoken command is the consent; opt out with REGENT_VOICE_AUTO_APPROVE=0.
        .env(
            "REGENT_AUTO_APPROVE",
            if env_flag("REGENT_VOICE_AUTO_APPROVE", true) {
                "1"
            } else {
                "0"
            },
        )
        // Spoken, conversational replies — see the deacon's voice_line().
        .env("REGENT_VOICE", "1");
    if env_flag("REGENT_VOICE_COMPUTER_USE", true) {
        cmd.env("REGENT_COMPUTER_USE", "1");
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return off(&format!("deacon spawn failed: {e}")),
    };
    let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
        return off("deacon pipes unavailable");
    };
    // Keep the child handle alive for the process lifetime (kill_on_drop).
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    let rpc = DeaconRpc::from_io(stdout, stdin);
    if rpc
        .call("health", json!({}), Duration::from_secs(30))
        .await
        .is_none()
    {
        return off("deacon didn't answer on stdio in 30s");
    }
    if rpc.ensure_session().await.is_none() {
        return off("deacon couldn't create a session");
    }
    AgentStatus::Ready(rpc)
}

#[cfg(test)]
mod tests {
    use super::{brain_env_from, call_model_from};

    #[test]
    fn provider_specific_key_is_preserved_without_generic_alias() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".env"), "NVIDIA_API_KEY=nvidia-secret\n").unwrap();
        let env = brain_env_from(home.path());
        assert_eq!(
            env.get("NVIDIA_API_KEY").map(String::as_str),
            Some("nvidia-secret")
        );
        assert!(!env.contains_key("REGENT_API_KEY"));
    }

    #[test]
    fn configured_call_model_overrides_only_the_voice_child() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.yaml"),
            "model:\n  default: legacy-model\nagents_defaults:\n  primary: nvidia/main\nspeech:\n  call:\n    fast_model: nvidia/fast\n",
        )
        .unwrap();
        let env = brain_env_from(home.path());
        assert_eq!(call_model_from(home.path()).as_deref(), Some("nvidia/fast"));
        assert!(!env.contains_key("REGENT_MODEL"));
        assert!(!env.contains_key("REGENT_BASE_URL"));
    }

    #[test]
    fn blank_call_model_does_not_freeze_the_legacy_default_into_the_voice_child() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.yaml"),
            "model:\n  default: legacy-model\n  provider: anthropic\n  base_url: https://legacy.example/v1\nagents_defaults:\n  primary:\n    provider: nvidia\n    model: nvidia/main\nspeech:\n  call:\n    fast_model: ''\n",
        )
        .unwrap();
        let env = brain_env_from(home.path());
        assert!(!env.contains_key("REGENT_MODEL"));
        assert!(!env.contains_key("REGENT_PROVIDER"));
        assert!(!env.contains_key("REGENT_BASE_URL"));
        assert_eq!(
            call_model_from(home.path()).as_deref(),
            Some("nvidia/nvidia/main")
        );
    }

    #[test]
    fn call_model_tracks_chat_provider_and_model_changes_without_a_restart() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("config.yaml");
        std::fs::write(
            &path,
            "agents_defaults:\n  primary:\n    provider: nvidia\n    model: nvidia/first\nspeech:\n  call:\n    fast_model: ''\n",
        )
        .unwrap();
        assert_eq!(
            call_model_from(home.path()).as_deref(),
            Some("nvidia/nvidia/first")
        );

        std::fs::write(
            path,
            "agents_defaults:\n  primary:\n    provider: openrouter\n    model: openai/gpt-next\nspeech:\n  call:\n    fast_model: ''\n",
        )
        .unwrap();
        assert_eq!(
            call_model_from(home.path()).as_deref(),
            Some("openrouter/openai/gpt-next")
        );
    }
}
