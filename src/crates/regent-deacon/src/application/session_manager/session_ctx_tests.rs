//! `ConfigGatedApprover` behavior: the live `tools.auto_approve` flag approves
//! tool gates without prompting, never swallows `ask_user`, and restores the
//! RPC prompt path the moment it's flipped off.

use super::*;
use regent_tools::ApprovalHandler;
use tokio::sync::mpsc::unbounded_channel;

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
