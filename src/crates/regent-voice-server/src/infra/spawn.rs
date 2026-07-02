//! Spawning the agent deacon for voice: env contract ported from
//! web_call.py's `_ensure_rpc` — the caller drives by voice and can't tap
//! "approve", so tool actions are auto-approved unless opted out, the session
//! answers in spoken style (`REGENT_VOICE=1`), and computer-use is on by
//! default so "look at my screen / open this site" works.

use crate::infra::deacon::{DeaconRpc, find_deacon};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
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

fn regent_home() -> PathBuf {
    if let Ok(h) = std::env::var("REGENT_HOME") {
        return PathBuf::from(h);
    }
    let user = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(user).join(".regent")
}

/// Backfill the agent's env from `$REGENT_HOME/.env` + config.yaml (model id
/// and base URL) — the same fallback the CLI's `brainEnv` injects — so a
/// MANUALLY started server still gets the full agent brain instead of the
/// echo. The real environment always wins.
fn brain_env() -> HashMap<String, String> {
    let home = regent_home();
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
    if let Ok(cfg) = std::fs::read_to_string(home.join("config.yaml"))
        && let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&cfg)
    {
        for (yaml_key, env_key) in [("default", "REGENT_MODEL"), ("base_url", "REGENT_BASE_URL")] {
            if let Some(v) = doc
                .get("model")
                .and_then(|m| m.get(yaml_key))
                .and_then(|v| v.as_str())
            {
                extra.insert(env_key.into(), v.to_owned());
            }
        }
    }
    // Process env wins over every backfilled value.
    extra.retain(|key, _| std::env::var(key).is_err());
    extra
}

/// Effective value: process env first, then the backfill map.
fn effective(extra: &HashMap<String, String>, key: &str) -> String {
    std::env::var(key)
        .ok()
        .or_else(|| extra.get(key).cloned())
        .unwrap_or_default()
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
    let model = {
        let m = effective(&extra, "REGENT_MODEL");
        if m.is_empty() {
            effective(&extra, "REGENT_BRAIN_MODEL")
        } else {
            m
        }
    };
    if effective(&extra, "REGENT_API_KEY").is_empty() || model.is_empty() {
        return off(&format!(
            "no model/key in env or {}\\.env — run `regent setup`",
            regent_home().display()
        ));
    }
    let mut cmd = Command::new(&deacon);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .envs(&extra)
        .env("REGENT_MODEL", model)
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
