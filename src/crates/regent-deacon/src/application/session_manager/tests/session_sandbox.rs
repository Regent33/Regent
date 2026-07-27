//! Unit tests for the session-context SECURITY decisions: where a session runs,
//! whether its paths are jailed, and whether its input is trusted. Split out of
//! `session_ctx.rs` (file-size rule) — the approval-handler tests stay there.
//! Same module tree via #[path], so `use super::*` still sees the parent.

use super::{SessionManager, is_untrusted_input, resolve_cwd, should_sandbox};
use std::sync::Arc;

/// A session with no workspace override runs where the deacon has always run
/// (the manager's boot cwd) — the CLI/platform path must not shift an inch.
/// A Desktop session that opened a folder runs THERE instead.
#[test]
fn resolve_cwd_prefers_the_session_workspace_over_the_default() {
    let default = std::path::Path::new("/deacon/boot/cwd");
    assert_eq!(
        resolve_cwd(default, None),
        default.to_path_buf(),
        "no override must resolve to the manager's cwd, byte for byte"
    );
    let opened = std::path::Path::new("/home/dev/my-project");
    assert_eq!(
        resolve_cwd(default, Some(opened)),
        opened.to_path_buf(),
        "an opened workspace must win over the default cwd"
    );
}

/// Opening a real project folder MUST jail the session to it. Local Desktop
/// sessions are otherwise unsandboxed, where `ToolContext::resolve` returns any
/// absolute path unchecked — tolerable only while the root is a disposable
/// artifacts dir. Once the root is the user's own repo, an unjailed session
/// puts their home dir, dotfiles, and sibling projects one bad absolute path
/// away, so `workspace.is_some()` has to be a sandbox trigger in its own right.
#[test]
fn a_session_that_opened_a_workspace_is_always_sandboxed() {
    assert!(
        should_sandbox(false, false, true, false),
        "an opened workspace alone must jail the session"
    );
    // The pre-existing triggers still stand on their own.
    assert!(
        should_sandbox(true, false, false, false),
        "external ingress stays jailed"
    );
    assert!(
        should_sandbox(false, true, false, false),
        "REGENT_SANDBOX stays honored"
    );
    // Default-on: an ordinary local session is jailed to its cwd too. It used
    // to be wide open — `ToolContext::resolve` returns ANY absolute path when
    // unsandboxed — so a hallucinated or injected path could edit anything on
    // the machine. Being in the sandbox folder was never a containment
    // mechanism, only a convention about where files usually landed.
    assert!(
        should_sandbox(false, false, false, false),
        "a plain local session is jailed by default"
    );
    // The escape hatch is explicit and never applies to the cases that must
    // stay jailed no matter what.
    assert!(
        !should_sandbox(false, false, false, true),
        "REGENT_UNSAFE_NO_SANDBOX opts a local session out"
    );
    assert!(
        should_sandbox(true, false, false, true),
        "external ingress cannot be opted out of the jail"
    );
    assert!(
        should_sandbox(false, false, true, true),
        "an opened workspace cannot be opted out of the jail"
    );
}

/// P0 regression matrix (`64aad1f` → fixed 2026-07-27, ADR-042). Jailing paths
/// and distrusting the input are SEPARATE decisions. `64aad1f` made the jail
/// default-on, and because two tools read the jail as "untrusted", every
/// ordinary session lost its local shell and its direct memory writes.
///
/// The pairing matters more than either row alone, so both are asserted here:
/// every case is (jailed?, untrusted?).
#[test]
fn the_path_jail_and_the_untrusted_marker_are_independent() {
    // (external, sandbox_env, workspace_set, unsafe_opt_out)
    let cases = [
        // An ordinary local session: jailed for safety, TRUSTED — it is the
        // user typing. This is the row the regression broke.
        ((false, false, false, false), true, false),
        // The user opening their own repo is still the user.
        ((false, false, true, false), true, false),
        // External ingress: jailed AND untrusted, both unconditional.
        ((true, false, false, false), true, true),
        ((true, false, false, true), true, true),
        // An explicit REGENT_SANDBOX run asks to be treated as untrusted.
        ((false, true, false, false), true, true),
        // The opt-out drops the path jail; trust is unaffected by it.
        ((false, false, false, true), false, false),
    ];
    for ((external, sandbox_env, workspace_set, opt_out), jailed, untrusted) in cases {
        assert_eq!(
            should_sandbox(external, sandbox_env, workspace_set, opt_out),
            jailed,
            "jail: external={external} env={sandbox_env} ws={workspace_set} opt_out={opt_out}"
        );
        assert_eq!(
            is_untrusted_input(external, sandbox_env),
            untrusted,
            "trust: external={external} env={sandbox_env}"
        );
    }
}

/// The pairing that caused the outage, named on its own so it cannot regress
/// quietly: jailed-and-trusted has to be a reachable state.
#[test]
fn an_ordinary_local_session_is_jailed_but_never_untrusted() {
    assert!(
        should_sandbox(false, false, false, false),
        "paths stay jailed by default — that rail is kept"
    );
    assert!(
        !is_untrusted_input(false, false),
        "but the user's own session must keep its shell and memory writes"
    );
}

/// The two tests above pin the pure decisions; this one pins that
/// `tool_context` actually APPLIES them. Without it, deleting the
/// `if untrusted { ctx.untrusted() }` line leaves every other test green —
/// which is exactly how the original conflation survived a full test suite.
#[test]
fn tool_context_marks_external_ingress_and_only_external_ingress() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(regent_store::Store::open(&dir.path().join("state.db")).unwrap());
    let graph = Arc::new(regent_graph::GraphMemory::new(Arc::clone(&store)));
    let skills = Arc::new(regent_skills::SkillLibrary::new(Arc::new(
        regent_skills::FsSkillRepository::new(dir.path().join("skills")).unwrap(),
    )));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let manager = SessionManager::new(
        Arc::new(|_model: &str| unreachable!("no provider is built for a tool context")),
        "scripted",
        store,
        graph,
        skills,
        dir.path().to_path_buf(),
        regent_agent::AgentConfig::default(),
        crate::domain::config::ToolsConfig::default(),
        tx,
    );

    let approval = || Arc::new(regent_tools::DenyAll) as Arc<dyn regent_tools::ApprovalHandler>;

    // An ordinary local session: jailed, and TRUSTED. The regression row.
    let local = manager.tool_context(false, approval(), None);
    assert!(local.is_sandboxed(), "local session keeps the path jail");
    assert!(
        !local.is_untrusted(),
        "the user's own session keeps its shell and memory writes"
    );

    // The user opening their own repo is still the user.
    let workspace = dir.path().join("project");
    std::fs::create_dir_all(&workspace).unwrap();
    let opened = manager.tool_context(false, approval(), Some(&workspace));
    assert!(opened.is_sandboxed(), "an opened workspace is jailed");
    assert!(!opened.is_untrusted(), "but it is not external ingress");

    // A keyed session IS external ingress: jailed AND untrusted.
    let external = manager.tool_context(true, approval(), None);
    assert!(external.is_sandboxed(), "external ingress stays jailed");
    assert!(
        external.is_untrusted(),
        "external ingress must be marked untrusted, or the tool gates fail open"
    );
}

/// What the jail actually buys, proven against the real `ToolContext` rather
/// than assumed: rooted at a workspace, a path outside it is refused while one
/// inside resolves. This is the mechanism the conditional above switches on.
#[test]
fn a_workspace_rooted_context_refuses_paths_outside_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("secrets.env");
    std::fs::write(&outside, "TOKEN=1").unwrap();

    let ctx = regent_tools::ToolContext::new_sandboxed(
        root.clone(),
        root.clone(),
        Arc::new(regent_tools::DenyAll),
    );
    assert!(
        ctx.resolve(&outside.display().to_string()).is_err(),
        "an absolute path outside the workspace must be refused"
    );
    assert!(
        ctx.resolve("src/main.rs").is_ok(),
        "a path inside the workspace must still resolve"
    );
}
