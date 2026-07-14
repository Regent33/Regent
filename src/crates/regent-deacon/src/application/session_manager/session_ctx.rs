//! Per-session approval handler + tool context construction. Split from
//! `lifecycle.rs` (file-size rule).

use super::SessionManager;
use super::hooks::{ApprovalTx, RpcApprovalHandler};
use super::lifecycle::SessionKind;
use crate::domain::errors::DeaconError;
use regent_agent::Agent;
use regent_kernel::SessionId;
use regent_tools::ToolContext;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

impl SessionManager {
    /// Approval handler for a new session. A surface with no way to prompt (a live
    /// voice call) sets `REGENT_AUTO_APPROVE=1` to approve automatically — opt-in,
    /// per dedicated deacon; otherwise approvals route to the client over RPC.
    /// On a voice deacon the auto-approver is scoped: GUI control the caller
    /// drives by voice (computer_use/control_app/browser/file edits) runs on
    /// spoken consent; only the unattended `terminal` shell stays denied;
    /// `REGENT_VOICE_FULL_CONTROL=1` opts back into blanket approval.
    pub(super) fn approval_handler(
        &self,
        sid_cell: &Arc<OnceLock<String>>,
        approval_pending: &Arc<Mutex<Option<ApprovalTx>>>,
    ) -> Arc<dyn regent_tools::ApprovalHandler> {
        let flag = |name: &str| {
            std::env::var(name)
                .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"))
                .unwrap_or(false)
        };
        let auto = flag("REGENT_AUTO_APPROVE");
        if auto {
            if flag("REGENT_VOICE") && !flag("REGENT_VOICE_FULL_CONTROL") {
                Arc::new(regent_tools::VoiceScopedApprover)
            } else {
                Arc::new(regent_tools::AllowAll)
            }
        } else {
            Arc::new(RpcApprovalHandler {
                session_id: Arc::clone(sid_cell),
                out_tx: self.out_tx.clone(),
                pending: Arc::clone(approval_pending),
            })
        }
    }

    /// Tool context for a session. Keyed sessions are external ingress
    /// (platform webhooks / gateway conversations), so they are always jailed
    /// to the workspace — an unauthorized or injected external turn must not
    /// reach `$REGENT_HOME/.env` or `~/.ssh`. `REGENT_SANDBOX` widens the
    /// jail to local sessions too; it can no longer narrow the external one.
    pub(super) fn tool_context(
        &self,
        external: bool,
        approval: Arc<dyn regent_tools::ApprovalHandler>,
    ) -> ToolContext {
        // Gap T6 spill area for oversized tool results — INSIDE the artifacts
        // subtree, so jailed sessions can still read_file the receipt.
        let artifacts = crate::application::http_serve::regent_home().join("artifacts");
        let scratch = artifacts.join("tool-output");
        if external || regent_tools::sandbox_enabled() {
            // The artifacts area is the ONE spot outside the jail a session may
            // write — the system prompt points every artifact/screenshot there,
            // and without this a jailed gateway session dumped them into its
            // cwd (the user saw a `.regent/` folder appear inside the repo).
            // Only the subtree: `$REGENT_HOME/.env` and state.db stay sealed.
            let _ = std::fs::create_dir_all(&artifacts);
            ToolContext::new_sandboxed(self.cwd.clone(), self.cwd.clone(), approval)
                .allow_subtree(artifacts)
                .with_scratch_dir(scratch)
        } else {
            ToolContext::new(self.cwd.clone(), approval).with_scratch_dir(scratch)
        }
    }

    pub(super) async fn create_session_keyed(
        &self,
        key: Option<&str>,
        kind: SessionKind,
        code_skill: Option<&str>,
    ) -> Result<SessionId, DeaconError> {
        let sid_cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
        let approval_pending: Arc<Mutex<Option<ApprovalTx>>> = Arc::new(Mutex::new(None));
        let approval = self.approval_handler(&sid_cell, &approval_pending);
        let provider = self.provider();
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
            .make_catalogs_and_prompt(&provider, &sid_cell, key, skill_overlay.as_deref())
            .await?;
        // Plan-mode (the `code.plan` read-only phase): restrict to the read-only
        // subset so the plan turn physically cannot edit — write/terminal tools
        // are absent from its catalog, not merely discouraged by the prompt.
        if kind == SessionKind::CodePlan {
            let names: Vec<String> = catalog.definitions().into_iter().map(|d| d.name).collect();
            catalog.restrict_to(&regent_code::plan_toolset(regent_code::Phase::Plan, &names));
        }
        // Gap T4: code sessions run unattended, so blocking questions need a
        // tool; chat already has the human in the loop (and the chat catalog
        // sits against its SPL token gate). Registered AFTER the plan
        // restriction — `ask_user` belongs in plan phase too (clarify before
        // planning beats guessing).
        if kind != SessionKind::Chat {
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
        let ctx = self.tool_context(key.is_some(), approval);
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
        .with_background_review(Self::review_setup(review_catalog))
        .with_delta_sink(self.delta_sink(&sid_cell));

        let id = agent.session_id().clone();
        let _ = sid_cell.set(id.to_string());
        self.entries
            .lock()
            .await
            .insert(id.clone(), self.make_entry(agent, approval_pending, ledger));
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
