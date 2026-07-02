//! Spawning the agent deacon for voice: env contract ported from
//! web_call.py's `_ensure_rpc` — the caller drives by voice and can't tap
//! "approve", so tool actions are auto-approved unless opted out, the session
//! answers in spoken style (`REGENT_VOICE=1`), and computer-use is on by
//! default so "look at my screen / open this site" works.

use crate::infra::deacon::{DeaconRpc, find_deacon};
use serde_json::json;
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
    let model = std::env::var("REGENT_MODEL")
        .or_else(|_| std::env::var("REGENT_BRAIN_MODEL"))
        .unwrap_or_default();
    if std::env::var("REGENT_API_KEY")
        .unwrap_or_default()
        .is_empty()
        || model.is_empty()
    {
        return off("no model/key in env");
    }
    let mut cmd = Command::new(&deacon);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
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
