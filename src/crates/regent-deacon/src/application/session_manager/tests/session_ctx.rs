//! `ConfigGatedApprover` behavior: the live `tools.auto_approve` flag approves
//! tool gates without prompting, never swallows `ask_user`, and restores the
//! RPC prompt path the moment it's flipped off.

use super::{ConfigGatedApprover, env_auto_approver};
use crate::application::session_manager::hooks::{ApprovalTx, RpcApprovalHandler};
use regent_tools::{ApprovalDecision, ApprovalHandler};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, mpsc::unbounded_channel};

fn gated(
    auto: bool,
) -> (
    ConfigGatedApprover,
    tokio::sync::mpsc::UnboundedReceiver<String>,
    Arc<Mutex<Option<ApprovalTx>>>,
) {
    let (out_tx, out_rx) = unbounded_channel();
    let pending: Arc<Mutex<Option<ApprovalTx>>> = Arc::new(Mutex::new(None));
    let sid: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
    let _ = sid.set("sess_test".to_owned());
    let approver = ConfigGatedApprover {
        auto: Arc::new(AtomicBool::new(auto)),
        inner: RpcApprovalHandler {
            session_id: sid,
            out_tx,
            pending: Arc::clone(&pending),
        },
    };
    (approver, out_rx, pending)
}

/// Drive a passthrough request: assert the `approval.request` notification is
/// emitted, then resolve the pending oneshot with `answer`.
async fn passthrough(
    approver: ConfigGatedApprover,
    mut out_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    pending: Arc<Mutex<Option<ApprovalTx>>>,
    tool: &'static str,
    answer: (bool, Option<String>),
) -> regent_tools::ApprovalDecision {
    let fut = tokio::spawn(async move { approver.request(tool, "act", "why").await });
    let line = out_rx.recv().await.expect("approval.request emitted");
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["method"], "approval.request");
    assert_eq!(v["params"]["tool"], tool);
    let tx = pending.lock().await.take().expect("pending stashed");
    tx.send(answer).unwrap();
    fut.await.unwrap()
}

#[tokio::test]
async fn auto_mode_approves_without_prompting() {
    let (approver, mut out_rx, _pending) = gated(true);
    let decision = approver
        .request("terminal", "rm -rf build", "dangerous")
        .await;
    assert!(matches!(decision, ApprovalDecision::Approve));
    // No RPC round-trip happened: nothing was emitted.
    assert!(out_rx.try_recv().is_err());
}

#[tokio::test]
async fn auto_mode_still_routes_ask_user_to_the_human() {
    let (approver, out_rx, pending) = gated(true);
    let decision = passthrough(
        approver,
        out_rx,
        pending,
        "ask_user",
        (false, Some("spaces, 2".to_owned())),
    )
    .await;
    assert!(matches!(decision, ApprovalDecision::DenyWithFeedback(f) if f == "spaces, 2"));
}

#[tokio::test]
async fn off_means_the_rpc_prompt_path_runs() {
    let (approver, out_rx, pending) = gated(false);
    let decision = passthrough(approver, out_rx, pending, "terminal", (true, None)).await;
    assert!(matches!(decision, ApprovalDecision::Approve));
}

#[tokio::test]
async fn flag_is_live_per_request() {
    let (approver, mut out_rx, _pending) = gated(false);
    approver.auto.store(true, Ordering::Release);
    let decision = approver.request("delete_file", "x", "y").await;
    assert!(matches!(decision, ApprovalDecision::Approve));
    assert!(out_rx.try_recv().is_err());
}

/// Env mutation must serialize: `env_auto_approver` reads fixed-name vars, so
/// tests can't dodge collisions with unique names. Held only across the
/// synchronous set→select→restore window below, never across an `.await`.
static ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Apply `vars` (each `Some` sets, `None` removes), run the selector, then
/// restore the prior values. Serialized and snapshot-restored so a parallel
/// test never observes a leaked var. The selector returns an `Arc` that no
/// longer touches the env, so the guard is dropped before any `.await`.
fn select_with_env(
    vars: &[(&str, Option<&str>)],
) -> Option<Arc<dyn regent_tools::ApprovalHandler>> {
    let _guard = ENV_GUARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let prior: Vec<(String, Option<String>)> = vars
        .iter()
        .map(|(k, _)| ((*k).to_owned(), std::env::var(k).ok()))
        .collect();
    for &(k, v) in vars {
        match v {
            Some(val) => unsafe { std::env::set_var(k, val) },
            None => unsafe { std::env::remove_var(k) },
        }
    }
    let handler = env_auto_approver();
    for (k, v) in &prior {
        match v {
            Some(val) => unsafe { std::env::set_var(k, val) },
            None => unsafe { std::env::remove_var(k) },
        }
    }
    handler
}

/// Default live voice (`REGENT_AUTO_APPROVE=1` + `REGENT_VOICE=1`, no full
/// control) denies EVERY mutation — the shell, desktop control, file edits, and
/// app/browser control all fail closed because the caller can't veto a prompt.
#[tokio::test]
async fn default_voice_denies_every_mutation() {
    let handler = select_with_env(&[
        ("REGENT_AUTO_APPROVE", Some("1")),
        ("REGENT_VOICE", Some("1")),
        ("REGENT_VOICE_FULL_CONTROL", None),
    ])
    .expect("REGENT_AUTO_APPROVE=1 selects an env posture");
    for tool in [
        "terminal",
        "computer_use",
        "write_file",
        "control_app",
        "browser_click",
    ] {
        assert_eq!(
            handler.request(tool, "act", "voice command").await,
            ApprovalDecision::Deny,
            "{tool} must be denied on a default live voice call"
        );
    }
}

/// `REGENT_VOICE_FULL_CONTROL=1` is the explicit opt-in: the same mutations are
/// approved so hands-on voice control still works.
#[tokio::test]
async fn full_control_voice_approves_every_mutation() {
    let handler = select_with_env(&[
        ("REGENT_AUTO_APPROVE", Some("1")),
        ("REGENT_VOICE", Some("1")),
        ("REGENT_VOICE_FULL_CONTROL", Some("1")),
    ])
    .expect("REGENT_AUTO_APPROVE=1 selects an env posture");
    for tool in ["terminal", "computer_use", "write_file", "control_app"] {
        assert_eq!(
            handler.request(tool, "act", "voice command").await,
            ApprovalDecision::Approve,
            "{tool} must be approved under REGENT_VOICE_FULL_CONTROL"
        );
    }
}

/// Non-voice auto mode is unchanged: `REGENT_AUTO_APPROVE=1` without
/// `REGENT_VOICE` still grants blanket approval (a human watches the surface).
#[tokio::test]
async fn non_voice_auto_still_approves_everything() {
    let handler = select_with_env(&[
        ("REGENT_AUTO_APPROVE", Some("1")),
        ("REGENT_VOICE", None),
        ("REGENT_VOICE_FULL_CONTROL", None),
    ])
    .expect("REGENT_AUTO_APPROVE=1 selects an env posture");
    assert_eq!(
        handler.request("terminal", "act", "auto").await,
        ApprovalDecision::Approve,
        "non-voice auto mode must remain blanket approval"
    );
}

/// No `REGENT_AUTO_APPROVE` → no env posture; the caller falls through to build
/// the config-gated RPC prompt path.
#[test]
fn no_auto_approve_env_yields_no_posture() {
    assert!(
        select_with_env(&[("REGENT_AUTO_APPROVE", None)]).is_none(),
        "without REGENT_AUTO_APPROVE the RPC prompt path is used"
    );
}
