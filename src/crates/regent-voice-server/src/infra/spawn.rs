//! Spawning the agent deacon for voice: env contract ported from
//! web_call.py's `_ensure_rpc`. The caller drives by voice and can't tap
//! "approve", so the deacon runs in auto mode (`REGENT_AUTO_APPROVE=1`, no RPC
//! prompt round-trip) and DENIES every mutation by default — a misheard command
//! must not act unseen. `REGENT_VOICE_FULL_CONTROL=1` opts into hands-on
//! control. Replies are spoken (`REGENT_VOICE=1`), and computer-use is
//! registered by default so read-only "look at my screen" works now and full
//! control needs no rebuild.
//!
//! Each candidate binary (see `infra::locate`) is spawned, health-probed and
//! asked for its first session in order; the first that answers wins and a
//! loser is killed AND reaped before the next is tried. A stale pinned
//! `REGENT_DEACON_PATH` therefore no longer leaves Butler mute — it costs one
//! failed probe and the reason is reported.

mod config;

#[cfg(test)]
#[path = "spawn_tests.rs"]
mod tests;

use crate::infra::deacon::DeaconRpc;
use crate::infra::locate::deacon_candidates;
use config::brain_env;
pub(crate) use config::{call_model_from, regent_home};
use serde_json::json;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};

/// Per-candidate health ceiling. A healthy deacon answers in well under a
/// second; a doomed one exits (stdout closes) or never answers. Kept generous
/// enough for a cold, AV-scanned first spawn, but short enough that falling
/// through to the next candidate still beats the caller's own patience.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(15);

/// Bounds on the aggregate failure text `/health` shows: a handful of reasons is
/// actionable, a whole PATH walk is not.
const MAX_REPORTED_FAILURES: usize = 4;
const MAX_REASON_CHARS: usize = 160;

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

/// Spawn the first working deacon among the resolved candidates and open its
/// session. The winning child dies with this process (`kill_on_drop`).
pub async fn spawn_agent() -> AgentStatus {
    if !env_flag("REGENT_VOICE_AGENT", true) {
        return AgentStatus::Unavailable("REGENT_VOICE_AGENT disabled".to_owned());
    }
    let candidates = deacon_candidates();
    if candidates.is_empty() {
        return AgentStatus::Unavailable(
            "regent-deacon binary not found (set REGENT_DEACON_PATH or build it with \
             `cargo build -p regent-deacon`)"
                .to_owned(),
        );
    }
    match first_healthy(
        &candidates,
        |path| async move { probe_candidate(&path).await },
    )
    .await
    {
        Ok(rpc) => AgentStatus::Ready(rpc),
        Err(reason) => AgentStatus::Unavailable(reason),
    }
}

/// Walk `candidates` in order and return the first probe that succeeds, else a
/// bounded aggregate naming what each one did. Exactly one pass — no retries,
/// no candidate visited twice.
async fn first_healthy<T, F, P>(candidates: &[PathBuf], mut probe: P) -> Result<T, String>
where
    P: FnMut(PathBuf) -> F,
    F: Future<Output = Result<T, String>>,
{
    let mut failures: Vec<String> = Vec::new();
    for candidate in candidates {
        match probe(candidate.clone()).await {
            Ok(ready) => return Ok(ready),
            Err(reason) => failures.push(format!(
                "{}: {}",
                candidate.display(),
                clip(&reason, MAX_REASON_CHARS)
            )),
        }
    }
    let hidden = failures.len().saturating_sub(MAX_REPORTED_FAILURES);
    failures.truncate(MAX_REPORTED_FAILURES);
    let more = if hidden == 0 {
        String::new()
    } else {
        format!(" (+{hidden} more)")
    };
    Err(format!(
        "no working regent-deacon among {} candidate(s): {}{more}",
        candidates.len(),
        failures.join("; ")
    ))
}

/// Single-line, length-capped reason text (a deacon's error could be long).
fn clip(text: &str, max: usize) -> String {
    let flat = text.replace(['\n', '\r'], " ");
    let flat = flat.trim();
    if flat.chars().count() <= max {
        return flat.to_owned();
    }
    flat.chars().take(max).collect::<String>() + "…"
}

/// Spawn one candidate, health-probe it, and open its first session. The child
/// handle is retained until BOTH succeed, so any failure kills and reaps it here
/// instead of leaving an orphan behind for the next candidate to compete with.
async fn probe_candidate(deacon: &Path) -> Result<Arc<DeaconRpc>, String> {
    let mut child = deacon_command(deacon)
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))?;
    let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
        discard(child).await;
        return Err("stdio pipes unavailable".to_owned());
    };
    let rpc = DeaconRpc::from_io(stdout, stdin);
    if rpc
        .call("health", json!({}), HEALTH_TIMEOUT)
        .await
        .is_none()
    {
        discard(child).await;
        return Err(format!(
            "didn't answer on stdio in {}s",
            HEALTH_TIMEOUT.as_secs()
        ));
    }
    if rpc.ensure_session().await.is_none() {
        discard(child).await;
        return Err("couldn't create a session".to_owned());
    }
    // Winner only: keep the child handle alive for the process lifetime, so
    // kill_on_drop ties the deacon to this server rather than orphaning it.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(rpc)
}

/// The voice deacon's launch contract — see the module doc for why auto mode is
/// paired with deny-by-default mutations.
fn deacon_command(deacon: &Path) -> Command {
    let mut cmd = Command::new(deacon);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .envs(brain_env())
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
    // CREATE_NO_WINDOW — probing several candidates must not flash one console
    // window per attempt; stdio pipes are unaffected.
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    cmd
}

/// Kill a losing candidate and REAP it: dropping without waiting leaves a
/// zombie on POSIX, and `start_kill` alone never confirms the exit.
async fn discard(mut child: Child) {
    child.start_kill().ok();
    let _ = child.wait().await;
}
