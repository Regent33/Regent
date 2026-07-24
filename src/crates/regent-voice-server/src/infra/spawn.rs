//! Spawning the agent deacon for voice: env contract ported from
//! web_call.py's `_ensure_rpc`. The caller drives by voice and can't tap
//! "approve", so the deacon runs in auto mode (`REGENT_AUTO_APPROVE=1`, no RPC
//! prompt round-trip) and DENIES every mutation by default — a misheard command
//! must not act unseen. `REGENT_VOICE_FULL_CONTROL=1` opts into hands-on
//! control. Replies are spoken (`REGENT_VOICE=1`), and computer-use is
//! registered by default so read-only "look at my screen" works now and full
//! control needs no rebuild.

mod config;

use crate::infra::deacon::{DeaconRpc, find_deacon};
use config::brain_env;
pub(crate) use config::{call_model_from, regent_home};
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
    let extra = brain_env();
    let mut cmd = Command::new(&deacon);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .envs(&extra)
        // Auto mode = no RPC approval prompt (a voice call can't answer one);
        // the deacon then denies mutations by default. Force the prompt path
        // with REGENT_VOICE_AUTO_APPROVE=0 (a voice call then can't approve).
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
