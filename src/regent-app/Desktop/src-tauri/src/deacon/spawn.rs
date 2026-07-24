//! Spawn regent-deacon as a hidden stdio child, trying each candidate binary in
//! turn and returning the FIRST that answers a `health` probe. A candidate that
//! fails to launch, boots then dies, or never answers is captured (bounded early
//! stderr, so its `fatal: …` reason surfaces), killed, and skipped — so a stale
//! pinned REGENT_DEACON_PATH no longer strands the app on a dead pipe. Env,
//! hidden-window, and artifacts-cwd behavior are preserved from the single-spawn
//! version.

use super::env::{merged_env, regent_home};
use super::locate::deacon_candidates;
use super::rpc::DeaconRpc;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::task::JoinHandle;

/// Per-candidate health-probe ceiling. A healthy deacon answers in well under a
/// second; a doomed one exits (stdout closes → immediate reject) or never
/// answers. Generous for a cold, AV-scanned first spawn of a fresh build.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);

/// Cap on retained early stderr — enough for a fatal line plus a little context.
const STDERR_CAP: usize = 4_096;

/// Spawn the first healthy deacon among the resolved candidates. `emit` receives
/// every streamed notification line from the WINNING child (losing candidates
/// are muted so their teardown never reaches the webview).
pub async fn spawn(
    emit: impl Fn(Value) + Send + Sync + Clone + 'static,
) -> Result<(Arc<DeaconRpc>, Child), String> {
    spawn_with(deacon_candidates(), regent_home(), emit).await
}

/// Candidate/home-injected core (tests drive it with a scratch home and an
/// explicit candidate list — no process-global env mutation).
pub(crate) async fn spawn_with(
    candidates: Vec<PathBuf>,
    home: PathBuf,
    emit: impl Fn(Value) + Send + Sync + Clone + 'static,
) -> Result<(Arc<DeaconRpc>, Child), String> {
    if candidates.is_empty() {
        return Err("regent-deacon binary not found (set REGENT_DEACON_PATH or build it with \
                    `cargo build -p regent-deacon`)"
            .to_string());
    }
    std::fs::create_dir_all(&home)
        .map_err(|e| format!("create REGENT_HOME {}: {e}", home.display()))?;
    // App sessions work in $REGENT_HOME/artifacts, not the GUI's inherited cwd
    // (which under `bun tauri dev` is the repo — a code task once scaffolded a
    // new project INSIDE the checkout). Everything produced lands in the same
    // subtree the prompt points at and the external-session jail allows.
    let artifacts = home.join("artifacts");
    std::fs::create_dir_all(&artifacts)
        .map_err(|e| format!("create artifacts {}: {e}", artifacts.display()))?;

    let mut attempts = Vec::new();
    for path in &candidates {
        match probe_candidate(path, &artifacts, &home, emit.clone()).await {
            Ok(pair) => return Ok(pair),
            Err(reason) => attempts.push(format!("{}: {reason}", path.display())),
        }
    }
    Err(format!(
        "no working regent-deacon among {} candidate(s):\n  {}",
        candidates.len(),
        attempts.join("\n  ")
    ))
}

/// Spawn one candidate, health-probe it, and return the connected client on
/// success; on failure kill the child and return the reason (its captured
/// stderr when present, else the probe error).
async fn probe_candidate(
    deacon: &Path,
    cwd: &Path,
    home: &Path,
    emit: impl Fn(Value) + Send + Sync + Clone + 'static,
) -> Result<(Arc<DeaconRpc>, Child), String> {
    let mut cmd = Command::new(deacon);
    cmd.current_dir(cwd);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Piped (not inherited) so we can capture a failed candidate's fatal
        // line; the reader still echoes to our stderr for dev-console parity.
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    apply_env(&mut cmd, home);
    // CREATE_NO_WINDOW — a hidden child must never flash a console window; the
    // repo has been burned by visible/focus-stealing consoles (see CHANGELOG).
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))?;
    let (Some(stdout), Some(stdin), Some(stderr)) =
        (child.stdout.take(), child.stdin.take(), child.stderr.take())
    else {
        child.kill().await.ok();
        return Err("deacon stdio pipes were not created".into());
    };
    let (captured, stderr_task) = capture_stderr(stderr);

    // Mute this candidate's notifications until it wins the probe, so a losing
    // child's `deacon.exited` (emitted when we kill it) never confuses the UI.
    let active = Arc::new(AtomicBool::new(false));
    let gate = Arc::clone(&active);
    let gated = move |line: Value| {
        if gate.load(Ordering::SeqCst) {
            emit(line);
        }
    };
    let rpc = DeaconRpc::from_io(stdout, stdin, gated);

    match rpc
        .request_with_timeout("health", json!({}), HEALTH_TIMEOUT)
        .await
    {
        Ok(_) => {
            active.store(true, Ordering::SeqCst);
            drop(stderr_task); // detached: keep draining the healthy child
            Ok((rpc, child))
        }
        Err(error) => {
            child.kill().await.ok();
            let _ = child.wait().await;
            let _ = stderr_task.await;
            let why = captured.lock().unwrap().trim().to_string();
            Err(if why.is_empty() { error } else { why })
        }
    }
}

/// Drain a child's stderr into a bounded buffer while echoing each line to our
/// own stderr (dev-console parity with the old `Stdio::inherit`).
fn capture_stderr(stderr: ChildStderr) -> (Arc<StdMutex<String>>, JoinHandle<()>) {
    let buffer = Arc::new(StdMutex::new(String::new()));
    let sink = Arc::clone(&buffer);
    let task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            eprintln!("{line}");
            let mut captured = sink.lock().unwrap();
            let combined = format!("{captured}{line}\n");
            *captured = combined
                .chars()
                .rev()
                .take(STDERR_CAP)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
        }
    });
    (buffer, task)
}

fn apply_env(cmd: &mut Command, home: &Path) {
    cmd.envs(merged_env(home));
}
