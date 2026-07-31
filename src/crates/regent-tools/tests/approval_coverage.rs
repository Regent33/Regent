//! Executable invariants for the approval boundary (plan A.5).
//!
//! The audit verified that `ApprovalDecision` is fail-closed *by type*. It never
//! verified *coverage* — which registered tools actually consult the gate.
//!
//! Two different strengths of check live here, and conflating them would be the
//! easiest way to end up with false confidence:
//!
//! * `every_registered_core_tool_has_a_recorded_approval_posture` is an
//!   INVENTORY tripwire. It proves only that no tool is registered without
//!   someone having written down whether it is gated. It does not verify that
//!   the written-down answer is true.
//! * the `terminal` tests are a real BEHAVIOURAL check: denial actually stops
//!   execution, and an ordinary command actually still runs (so the first test
//!   cannot pass by the dispatch being broken).
//!
//! See `docs/audits/approval-sandbox-boundary-2026-07-31.md` for the map.

use async_trait::async_trait;
use regent_kernel::RegentError;
use regent_tools::application::registry::core_catalog_with_terminal;
use regent_tools::domain::contracts::{CommandOutput, TerminalBackend};
use regent_tools::{ApprovalDecision, ApprovalHandler, ToolContext, core_catalog};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// `std::env` is process-global; the env-reading tests must not overlap.
static ENV_LOCK: Mutex<()> = Mutex::new(());
use std::time::Duration;

/// Denies everything and records whether it was asked at all.
struct RecordingDenier(Arc<AtomicBool>);

#[async_trait]
impl ApprovalHandler for RecordingDenier {
    async fn request(&self, _tool: &str, _action: &str, _reason: &str) -> ApprovalDecision {
        self.0.store(true, Ordering::SeqCst);
        ApprovalDecision::Deny
    }
}

/// A backend that records being asked to run anything and never runs it. This
/// is what makes non-execution observable: asserting on the returned string
/// would only prove a denial was *reported*, not that nothing ran.
struct RecordingBackend(Arc<AtomicBool>);

#[async_trait]
impl TerminalBackend for RecordingBackend {
    fn describe(&self) -> String {
        "recording".to_owned()
    }
    async fn run(
        &self,
        _command: &str,
        _cwd: &std::path::Path,
        _timeout: Duration,
    ) -> Result<CommandOutput, RegentError> {
        self.0.store(true, Ordering::SeqCst);
        Err(RegentError::Config("must not be reached".to_owned()))
    }
}

/// Every tool `core_catalog()` registers, classified by whether it consults the
/// approval gate. A tool added or renamed without a decision fails this test —
/// that is the entire point. `false` is not an accusation; it records that the
/// tool runs unprompted, which is a choice someone has to have made on purpose.
const GATED: &[&str] = &[
    "terminal",     // only when detect_dangerous_command matches
    "control_app",  // every action
    "computer_use", // mutating actions (registered only with REGENT_COMPUTER_USE=1)
];

const UNGATED: &[&str] = &[
    // Read-only.
    "read_file",
    "read_document",
    "search_files",
    "glob",
    "ls",
    "current_time",
    "web_search",
    "web_fetch",
    "vision_analyze",
    "video_analyze",
    "camera_capture",
    // Everyday toolset — pure computation or a read-only third-party lookup.
    "calc",
    "convert",
    "date_calc",
    "dictionary",
    "qr_code",
    "random_gen",
    "reminder",
    "sun_moon",
    "weather",
    "world_time",
    // MUTATING AND UNGATED. Each of these writes to the workspace or acts
    // outward without ever reaching the approval handler.
    "write_file",
    "file_edit",
    "apply_patch",
    "create_document",
    "image_generation",
    "open_url",
    "play",
];

#[test]
fn every_registered_core_tool_has_a_recorded_approval_posture() {
    let registered: BTreeSet<String> = core_catalog()
        .definitions()
        .into_iter()
        .map(|d| d.name)
        .collect();
    let classified: BTreeSet<String> = GATED
        .iter()
        .chain(UNGATED)
        .map(|s| (*s).to_owned())
        .collect();

    let unclassified: Vec<_> = registered.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "tool(s) registered with no recorded approval posture: {unclassified:?}. \
         Decide whether each one is gated, then add it to GATED or UNGATED in this file."
    );

    // computer_use is conditional on REGENT_COMPUTER_USE, so absence is fine;
    // anything else in the table that is not registered has been renamed.
    let stale: Vec<_> = classified
        .difference(&registered)
        .filter(|n| n.as_str() != "computer_use")
        .collect();
    assert!(
        stale.is_empty(),
        "recorded tool(s) no longer registered — renamed or removed? {stale:?}"
    );
}

/// The one gate that has to hold under an unattended posture: a destructive
/// shell command is routed to the handler, and a denial stops it reaching the
/// terminal backend at all.
#[tokio::test]
async fn a_dangerous_terminal_command_is_denied_without_executing() {
    let asked = Arc::new(AtomicBool::new(false));
    let ran = Arc::new(AtomicBool::new(false));
    let ctx = ToolContext::new(
        std::env::temp_dir(),
        Arc::new(RecordingDenier(Arc::clone(&asked))),
    );
    let catalog = core_catalog_with_terminal(Arc::new(RecordingBackend(Arc::clone(&ran))));

    // Recursive force-delete matches the dangerous-command patterns.
    let args = serde_json::json!({ "command": "rm -rf ./some-directory" }).to_string();
    let result = catalog.dispatch("terminal", &args, &ctx).await;

    assert!(
        asked.load(Ordering::SeqCst),
        "a destructive terminal command never reached the approval gate"
    );
    assert!(
        !ran.load(Ordering::SeqCst),
        "a DENIED command still reached the terminal backend: {result}"
    );
    assert!(
        result.contains("denied"),
        "denied command did not report a denial: {result}"
    );
}

/// The converse, and the reason the test above is not vacuous: an ordinary
/// command is NOT gated and does reach the backend. Without this, deleting the
/// dispatch entirely would still pass.
#[tokio::test]
async fn an_ordinary_terminal_command_is_not_gated_and_does_reach_the_backend() {
    let asked = Arc::new(AtomicBool::new(false));
    let ran = Arc::new(AtomicBool::new(false));
    let ctx = ToolContext::new(
        std::env::temp_dir(),
        Arc::new(RecordingDenier(Arc::clone(&asked))),
    );
    let catalog = core_catalog_with_terminal(Arc::new(RecordingBackend(Arc::clone(&ran))));

    let args = serde_json::json!({ "command": "echo hello" }).to_string();
    let _ = catalog.dispatch("terminal", &args, &ctx).await;

    assert!(
        !asked.load(Ordering::SeqCst),
        "an ordinary command was gated"
    );
    assert!(
        ran.load(Ordering::SeqCst),
        "an ordinary command never reached the backend — is dispatch still wired?"
    );
}

/// `REGENT_SANDBOX=1` with the host `local` backend must be an ERROR, not a
/// quiet fallback. This is the enforcement that cron, the kanban workers and
/// `regent mcp serve` used to skip entirely by constructing their catalog with
/// `core_catalog()` — which never reaches this check.
///
/// Serialised with the other env-reading test: `std::env` is process-global.
#[test]
fn sandbox_opt_in_refuses_the_host_backend_rather_than_falling_back() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: guarded by ENV_LOCK; no other thread reads these while held.
    unsafe {
        std::env::set_var("REGENT_SANDBOX", "1");
        std::env::set_var("REGENT_TERMINAL_BACKEND", "local");
    }
    let refused = regent_tools::application::registry::core_catalog_from_env();
    unsafe {
        std::env::remove_var("REGENT_SANDBOX");
        std::env::remove_var("REGENT_TERMINAL_BACKEND");
    }
    let Err(error) = refused else {
        panic!("REGENT_SANDBOX=1 with the local backend must not silently succeed");
    };
    assert!(
        error.to_string().contains("REGENT_SANDBOX"),
        "the error must name the flag that caused it: {error}"
    );
}

/// The plain constructor is the one that skips enforcement — pinned so nobody
/// "fixes" a composition root by switching back to it.
#[test]
fn the_plain_constructor_is_the_one_without_enforcement() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var("REGENT_SANDBOX", "1");
    }
    // No error, because `core_catalog()` never consults the flag. That is
    // exactly why composition roots must not use it.
    let catalog = core_catalog();
    unsafe {
        std::env::remove_var("REGENT_SANDBOX");
    }
    assert!(!catalog.definitions().is_empty());
}
