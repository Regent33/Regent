//! ADR-038 P0 — measure-first prompt-profile routing. `profile.estimate`
//! renders BOTH candidate prompt profiles (`light`/`full`) for a would-be new
//! session and reports their sizes; `profile.report` combines that with a
//! session-mix telemetry read (`regent_store::Store::session_mix`) into the
//! analytic A-vs-B billed-token comparison (`profile_model`, split out for
//! the file-size rule) from the token-efficiency plan (§3.1, §8). Neither op
//! creates a session or changes any existing one — P0 is measurement only.

use super::Dispatcher;
use super::profile_model::{AGENTIC_SOURCE_HINTS, BilledInputs, CACHE_MODELS, DEACON_CACHE_MODEL, billed_tokens_a_vs_b};
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use crate::domain::errors::DeaconError;
use serde_json::{Value, json};

/// `{prompt_chars, prompt_est_tokens, defs_chars, defs_est_tokens,
/// total_est_tokens}` for one rendered profile — same chars/4 convention as
/// `context.budget` (`session_manager/queries.rs`).
fn profile_size_json((prompt, defs): &(String, String)) -> Value {
    let prompt_chars = prompt.len();
    let defs_chars = defs.len();
    json!({
        "prompt_chars": prompt_chars,
        "prompt_est_tokens": prompt_chars / 4,
        "defs_chars": defs_chars,
        "defs_est_tokens": defs_chars / 4,
        "total_est_tokens": (prompt_chars + defs_chars) / 4,
    })
}

fn profile_total_tokens((prompt, defs): &(String, String)) -> f64 {
    ((prompt.len() + defs.len()) / 4) as f64
}

impl Dispatcher {
    /// `profile.estimate` (no params): renders BOTH ADR-038 candidate
    /// profiles for a would-be new session and reports their sizes. No
    /// session is created; existing sessions are untouched.
    pub(super) async fn profile_estimate(&self, req: RpcRequest) {
        let light = self.sessions.fixed_prefix_for(true).await;
        let full = self.sessions.fixed_prefix_for(false).await;
        match (light, full) {
            (Ok(light), Ok(full)) => self.send(ok_response(
                req.id,
                json!({
                    "light": profile_size_json(&light),
                    "full": profile_size_json(&full),
                }),
            )),
            (Err(e), _) | (_, Err(e)) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    /// `profile.report` (`{days?: number}`, default 30): the session-mix
    /// A-vs-B billed-token comparison (ADR-038 P0(c)) — combines both prefix
    /// sizes, `Store::session_mix`, and `profile_model`'s analytic model per
    /// cache pricing model. `verdict` judges the Anthropic-5m model, the
    /// deacon's actual cadence policy (`cache_policy_for_source`).
    /// `sessions_analyzed == 0` short-circuits to `verdict:
    /// "insufficient-data"` — an empty window has no mix to route on.
    pub(super) async fn profile_report(&self, req: RpcRequest) {
        let days = req
            .params
            .get("days")
            .and_then(Value::as_f64)
            .unwrap_or(30.0);
        match self.build_profile_report(days).await {
            Ok(v) => self.send(ok_response(req.id, v)),
            Err(e) => self.send(err_response(req.id, -32000, e.to_string())),
        }
    }

    async fn build_profile_report(&self, days: f64) -> Result<Value, DeaconError> {
        let light = self.sessions.fixed_prefix_for(true).await?;
        let full = self.sessions.fixed_prefix_for(false).await?;
        let light_tokens = profile_total_tokens(&light);
        let full_tokens = profile_total_tokens(&full);

        let mix = self
            .sessions
            .store_handle()
            .session_mix(days)
            .map_err(DeaconError::Store)?;

        if mix.total_sessions == 0 {
            return Ok(json!({
                "inputs": {
                    "days": days,
                    "sessions_analyzed": 0,
                    "light_prompt_est_tokens": light_tokens,
                    "full_prompt_est_tokens": full_tokens,
                },
                "sessions_analyzed": 0,
                "models": {},
                "verdict": "insufficient-data",
            }));
        }

        let agentic_sessions: i64 = mix
            .by_source
            .iter()
            .filter(|s| {
                AGENTIC_SOURCE_HINTS
                    .iter()
                    .any(|hint| s.source.contains(hint))
            })
            .map(|s| s.session_count)
            .sum();
        let chat_sessions = mix.total_sessions - agentic_sessions;
        let total_turns: i64 = mix.by_source.iter().map(|s| s.total_turns).sum();
        let avg_turns = total_turns as f64 / mix.total_sessions as f64;

        let inputs = BilledInputs {
            light_tokens,
            full_tokens,
            escalation_share: mix.escalation_share,
            avg_turns,
            chat_sessions: chat_sessions as f64,
            agentic_sessions: agentic_sessions as f64,
        };

        let mut models = serde_json::Map::new();
        let mut verdict = "insufficient-data";
        for cache_model in CACHE_MODELS {
            let (a_billed, b_billed) = billed_tokens_a_vs_b(&inputs, cache_model);
            let winner = if a_billed <= b_billed { "A" } else { "B" };
            if cache_model.name == DEACON_CACHE_MODEL {
                verdict = winner;
            }
            models.insert(
                cache_model.name.to_owned(),
                json!({ "a_billed": a_billed, "b_billed": b_billed, "winner": winner }),
            );
        }

        // ADR-038 P2: the LIVE escalation rate (sessions born light that went
        // full) — distinct from `escalation_share` above, which is the P0
        // analytic proxy over tool calls. This is the drift alarm.
        let (light_sessions, escalated_sessions) = self
            .sessions
            .store_handle()
            .profile_stats(days)
            .map_err(DeaconError::Store)?;
        Ok(json!({
            "inputs": {
                "days": days,
                "sessions_analyzed": mix.total_sessions,
                "escalation_share": mix.escalation_share,
                "avg_turns": avg_turns,
                "chat_sessions": chat_sessions,
                "agentic_sessions": agentic_sessions,
                "light_prompt_est_tokens": light_tokens,
                "full_prompt_est_tokens": full_tokens,
            },
            "sessions_analyzed": mix.total_sessions,
            "models": models,
            "verdict": verdict,
            "live": {
                "light_sessions": light_sessions,
                "escalated_sessions": escalated_sessions,
                "escalation_rate": if light_sessions > 0 {
                    escalated_sessions as f64 / light_sessions as f64
                } else {
                    0.0
                },
            },
        }))
    }
}

#[cfg(test)]
#[path = "tests/profile_ops_tests.rs"]
mod tests;
