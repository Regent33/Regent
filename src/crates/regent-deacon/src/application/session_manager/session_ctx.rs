//! Per-session approval handler + tool context construction. Split from
//! `lifecycle.rs` (file-size rule).

mod approval;

use super::SessionManager;
use super::hooks::ApprovalTx;
use super::lifecycle::SessionKind;
use crate::domain::errors::DeaconError;
#[cfg(test)]
use approval::{ConfigGatedApprover, env_auto_approver};
use regent_agent::Agent;
use regent_kernel::SessionId;
use regent_tools::ToolContext;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

/// Truthy env flag ("1"/"true"/"TRUE"/"yes"), the ONE parser for every env
/// gate in this file — two hand-rolled copies drifting apart would let a
/// spelling count as "voice" for approvals but not for profile routing.
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false)
}

/// Where a session's tools resolve relative paths: its own opened workspace
/// when it has one, else the manager's boot cwd. Sessions without an override
/// (every CLI, platform, and background session) keep the historical behavior
/// exactly — this is the ONE place that choice is made.
fn resolve_cwd(default_cwd: &Path, workspace: Option<&Path>) -> PathBuf {
    workspace.unwrap_or(default_cwd).to_path_buf()
}

/// Whether a session's tools are jailed to their cwd. Pure so the security
/// decision is testable on its own (no `SessionManager` needed).
///
/// `workspace_set` is a trigger in its OWN right: a local session is otherwise
/// unsandboxed, and `ToolContext::resolve` then returns any absolute path
/// unchecked. That is only tolerable while the root is a disposable artifacts
/// dir — once the user opens their real repo, an unjailed session puts their
/// home dir, dotfiles, and sibling projects one bad absolute path away.
fn should_sandbox(
    external: bool,
    sandbox_env: bool,
    workspace_set: bool,
    unsafe_opt_out: bool,
) -> bool {
    // These two are never opt-out-able. External ingress is untrusted by
    // definition, and a folder the user opened is their real work.
    if external || workspace_set {
        return true;
    }
    // Otherwise jailed by DEFAULT. An unsandboxed context returns any absolute
    // path from `resolve()` unchecked, so a hallucinated or injected path could
    // reach anything on the machine; living in the artifacts folder was only
    // ever a convention about where files land, never containment.
    sandbox_env || !unsafe_opt_out
}

impl SessionManager {
    /// Tool context for a session. Keyed sessions are external ingress
    /// (platform webhooks / gateway conversations), so they are always jailed
    /// to the workspace — an unauthorized or injected external turn must not
    /// reach `$REGENT_HOME/.env` or `~/.ssh`. `REGENT_SANDBOX` widens the
    /// jail to local sessions too; it can no longer narrow the external one.
    pub(super) fn tool_context(
        &self,
        external: bool,
        approval: Arc<dyn regent_tools::ApprovalHandler>,
        workspace: Option<&Path>,
    ) -> ToolContext {
        // Gap T6 spill area for oversized tool results — INSIDE the artifacts
        // subtree, so jailed sessions can still read_file the receipt.
        let artifacts = crate::application::http_serve::regent_home().join("artifacts");
        let scratch = artifacts.join("tool-output");
        // A session that opened a project folder runs THERE, jailed to it.
        let cwd = resolve_cwd(&self.cwd, workspace);
        if should_sandbox(
            external,
            regent_tools::sandbox_enabled(),
            workspace.is_some(),
            env_flag("REGENT_UNSAFE_NO_SANDBOX"),
        ) {
            // The artifacts area is the ONE spot outside the jail a session may
            // write — the system prompt points every artifact/screenshot there,
            // and without this a jailed gateway session dumped them into its
            // cwd (the user saw a `.regent/` folder appear inside the repo).
            // Only the subtree: `$REGENT_HOME/.env` and state.db stay sealed.
            let _ = std::fs::create_dir_all(&artifacts);
            ToolContext::new_sandboxed(cwd.clone(), cwd, approval)
                .allow_subtree(artifacts.clone())
                .with_scratch_dir(scratch)
                .with_artifacts_dir(artifacts)
        } else {
            ToolContext::new(cwd, approval)
                .with_scratch_dir(scratch)
                .with_artifacts_dir(artifacts)
        }
    }

    pub(super) async fn create_session_keyed(
        &self,
        key: Option<&str>,
        kind: SessionKind,
        code_skill: Option<&str>,
        workspace: Option<PathBuf>,
    ) -> Result<SessionId, DeaconError> {
        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let approval_pending: Arc<Mutex<Option<ApprovalTx>>> = Arc::new(Mutex::new(None));
        let approval = self.approval_handler(&sid_cell, &approval_pending);
        let provider = self.provider();
        // ADR-038 P1: plain chat routes to the `light` profile. Everything
        // agentic — code phases, detached background jobs, a voice deacon
        // (spoken turns lean on the full tool loop) — stays full, as does
        // everything when the `tools.light_profile` kill-switch is off.
        let light = self.light_profile && kind == SessionKind::Chat && !env_flag("REGENT_VOICE");
        // Harness-skill seam (Wave 1c): resolve the named skill via the library
        // (disk overrides bundled) BEFORE building the prompt; an unknown name
        // is a hard error, never a silently skill-less run.
        let skill_overlay = match code_skill {
            Some(name) => {
                let record = self
                    .skills
                    .view(name)
                    .map_err(regent_kernel::RegentError::from)
                    .map_err(DeaconError::Core)?;
                Some(format!(
                    "\n\n## Active skill: {}\n\n{}",
                    record.meta.name, record.body
                ))
            }
            None => None,
        };
        let (mut catalog, review_catalog, mut ledger) = self
            .make_catalogs_and_prompt(&provider, &sid_cell, key, skill_overlay.as_deref(), light)
            .await?;
        // ADR-038 P2: a light session escalates (one-way) when the model
        // reaches for an agentic tool — the hook flips the flag, `run_turn`
        // rebuilds the prompt as full on the NEXT turn (never mid-turn; the
        // prompt is frozen per request). Full sessions carry no hook.
        let escalate_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if light {
            catalog.add_hook(Arc::new(super::hooks::EscalationHook {
                pending: Arc::clone(&escalate_pending),
            }));
        }
        // Plan-mode (the `code.plan` read-only phase): restrict to the read-only
        // subset so the plan turn physically cannot edit — write/terminal tools
        // are absent from its catalog, not merely discouraged by the prompt.
        if kind == SessionKind::CodePlan {
            // names() not definitions(): the allow-list must see deferred
            // tools too, or they'd be dropped instead of un-deferred.
            catalog.restrict_to(&regent_code::plan_toolset(
                regent_code::Phase::Plan,
                &catalog.names(),
            ));
        }
        // Gap T4: code sessions run unattended, so blocking questions need a
        // tool; chat already has the human in the loop (and the chat catalog
        // sits against its SPL token gate). Registered AFTER the plan
        // restriction — `ask_user` belongs in plan phase too (clarify before
        // planning beats guessing). Background jobs don't get it either: they
        // are unattended by definition, and a question would block forever.
        if matches!(kind, SessionKind::CodePlan | SessionKind::CodeExecute) {
            regent_tools::register_ask_user_tool(&mut catalog).map_err(DeaconError::Core)?;
        }
        // Code-execute sessions get edit-time diagnostics (gap H5) — the cheap
        // per-language check rides each edit's own result — plus the
        // `todo_write` working-plan tool (gap T2; code sessions only so the
        // chat catalog stays under its SPL token gate — chat has kanban).
        // Chat sessions are untouched.
        if kind == SessionKind::CodeExecute {
            regent_tools::register_todo_tool(&mut catalog).map_err(DeaconError::Core)?;
            regent_code::wrap_diagnostics(&mut catalog, &self.cwd);
        }
        // Seal AFTER disable/defer/restrict: the baseline must hash the
        // definitions exactly as this session sends them to the provider.
        ledger.seal(&serde_json::to_string(&catalog.definitions()).unwrap_or_default());
        let system_prompt = ledger.render();
        let ctx = self.tool_context(key.is_some(), approval, workspace.as_deref());
        let agent = Agent::new(
            Arc::clone(&provider),
            Arc::new(catalog),
            Arc::clone(&self.store),
            ctx,
            system_prompt,
            self.agent_config(),
        )
        .map_err(DeaconError::Core)?
        .with_graph_memory(Arc::clone(&self.graph))
        .with_background_review(self.review_setup(review_catalog))
        .with_delta_sink(self.delta_sink(&sid_cell));

        let id = agent.session_id().clone();
        let _ = sid_cell.set(id.to_string());
        // Escalation-rate telemetry (ADR-038 P2 ships WITH the feature): stamp
        // the birth profile; best-effort — a store miss must not kill creation.
        if let Err(e) = self
            .store
            .set_session_profile(&id, if light { "light" } else { "full" })
        {
            tracing::warn!(session = %id, error = %e, "profile stamp failed");
        }
        // Persist the opened folder so resuming after a restart re-opens the
        // same tree; best-effort for the same reason as the profile stamp.
        if let Some(root) = workspace.as_deref()
            && let Err(e) = self
                .store
                .set_session_workspace(&id, &root.display().to_string())
        {
            tracing::warn!(session = %id, error = %e, "workspace stamp failed");
        }
        self.entries.lock().await.insert(
            id.clone(),
            self.make_entry(
                agent,
                approval_pending,
                ledger,
                light,
                escalate_pending,
                key,
                workspace,
            ),
        );
        // Announce EVERY birth from the one place sessions are born, so the
        // session rail learns about code-plan/background/http sessions live —
        // `turn.started` only covers the prompt.submit path.
        let notification = crate::domain::entities::RpcNotification::new(
            "session.created",
            serde_json::json!({"session_id": id.to_string()}),
        );
        if let Ok(line) = serde_json::to_string(&notification) {
            self.out_tx.send(line).ok();
        }
        Ok(id)
    }
}

#[cfg(test)]
#[path = "tests/session_ctx.rs"]
mod tests;
