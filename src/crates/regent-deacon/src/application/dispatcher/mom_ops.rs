//! `mom.run` — Mixture-of-Models groups (§B): fan out proposers, synthesize
//! through the aggregator.

use super::Dispatcher;
use crate::application::provider_registry::ProviderRegistry;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use regent_agent::MomRunner;
use serde_json::json;

impl Dispatcher {
    /// `mom.run` — run a configured Mixture-of-Models group (§B): resolve its
    /// proposer + aggregator model specs through the registry, fan out the
    /// proposers (advisory), and return the aggregator's synthesis. Unresolvable
    /// proposers are skipped (logged); an unresolvable aggregator is a hard error.
    pub(super) fn mom_run(&self, req: RpcRequest) {
        let (Some(name), Some(brief)) = (
            req.params
                .get("name")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            req.params
                .get("brief")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
        ) else {
            self.send(err_response(req.id, -32602, "missing name or brief"));
            return;
        };
        // Resolve to an owned MomRunner synchronously, so no borrow of self/config
        // crosses the await below (keeps the future Send for the in-process tool).
        let runner = match self.prepare_mom(&name) {
            Ok(runner) => runner,
            Err((code, message)) => {
                self.send(err_response(req.id, code, message));
                return;
            }
        };
        // Detached: N proposer model calls must never block the serial read
        // loop (same rule as prompt.submit/code.start — everything queued
        // behind an inline await, incl. turn.interrupt, freezes the app).
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let resp = match runner.run(&brief).await {
                Ok(synthesis) => {
                    ok_response(req.id, json!({"group": name, "synthesis": synthesis}))
                }
                Err(error) => err_response(req.id, -32000, error.to_string()),
            };
            if let Ok(line) = serde_json::to_string(&resp) {
                out_tx.send(line).ok();
            }
        });
    }

    /// Build a `MomRunner` for a configured group, resolving its model specs
    /// through the provider registry. Sync (borrows self only here); typed
    /// `(code, message)` errors map straight to a JSON-RPC error.
    fn prepare_mom(&self, name: &str) -> Result<MomRunner, (i32, String)> {
        let cfg = self
            .config_snapshot()
            .ok_or((-32000, "config not wired".to_owned()))?;
        let group = cfg
            .mom
            .get(name)
            .ok_or((-32602, format!("unknown mom group '{name}'")))?;
        let registry = ProviderRegistry::from_config(&cfg.providers);
        let default = cfg.agents_defaults.primary.as_ref();
        let proposers: Vec<_> = group
            .proposers
            .iter()
            .filter_map(|spec| {
                let provider = registry
                    .resolve_model_str(spec, default)
                    .and_then(|m| registry.provider_for(&m).ok());
                if provider.is_none() {
                    tracing::warn!(spec, "mom proposer unresolved; skipping");
                }
                provider
            })
            .collect();
        let aggregator_ref = registry
            .resolve_model_str(&group.aggregator, default)
            .ok_or((
                -32602,
                format!(
                    "mom aggregator '{}' unresolved — configure a matching provider",
                    group.aggregator
                ),
            ))?;
        let aggregator = registry
            .provider_for(&aggregator_ref)
            .map_err(|e| (-32000, format!("mom aggregator: {e}")))?;
        let mut runner = MomRunner::new(proposers, aggregator);
        if group.max_proposers > 0 {
            runner = runner.with_max_proposers(group.max_proposers);
        }
        Ok(runner)
    }
}
