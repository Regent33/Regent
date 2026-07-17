//! Session-mix analytics (ADR-038 P0(c)): aggregates existing session/turn/
//! message telemetry into the inputs the light/full A-vs-B billed-token
//! model needs (`profile.report` in regent-deacon). Read-only, derived
//! entirely from tables the SPL already writes — no new instrumentation, no
//! behavior change.

use crate::domain::entities::{SessionMixReport, SourceMix};
use crate::domain::errors::StoreError;
use crate::infra::db::{Store, now_epoch};
use rusqlite::params;

impl Store {
    /// Session mix over the last `days` days: per-source turn/token
    /// aggregates plus the escalation share (fraction of sessions that
    /// called `code_task`/`delegate_task`/`load_tools` at least once).
    pub fn session_mix(&self, days: f64) -> Result<SessionMixReport, StoreError> {
        let cutoff = now_epoch() - days * 86_400.0;
        let by_source = self.with_read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT s.source,
                        COUNT(*) AS session_count,
                        COALESCE(SUM(
                            (SELECT COUNT(*) FROM turns t WHERE t.session_id = s.id)
                        ), 0) AS total_turns,
                        COALESCE(SUM(s.input_tokens), 0) AS total_input_tokens,
                        COALESCE(SUM(s.api_call_count), 0) AS total_api_calls
                 FROM sessions s
                 WHERE s.started_at >= ?1
                 GROUP BY s.source
                 ORDER BY s.source",
            )?;
            let rows = stmt.query_map(params![cutoff], |r| {
                let session_count: i64 = r.get(1)?;
                let total_turns: i64 = r.get(2)?;
                let total_input_tokens: i64 = r.get(3)?;
                let total_api_calls: i64 = r.get(4)?;
                Ok(SourceMix {
                    source: r.get(0)?,
                    session_count,
                    total_turns,
                    avg_turns_per_session: safe_div(total_turns as f64, session_count as f64),
                    total_input_tokens,
                    avg_input_tokens_per_call: safe_div(
                        total_input_tokens as f64,
                        total_api_calls as f64,
                    ),
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })?;

        let (total_sessions, escalating_sessions) = self.with_read(|conn| {
            let total: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE started_at >= ?1",
                params![cutoff],
                |r| r.get(0),
            )?;
            // One EXISTS subquery: a session escalates if any assistant
            // message's tool_calls (JSON text) mentions one of the three
            // escalation-trigger tools. Mirrors `ESCALATION_TRIGGERS` in
            // regent-deacon `session_manager/hooks.rs` by hand — change BOTH.
            let escalating: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions s
                 WHERE s.started_at >= ?1
                   AND EXISTS (
                       SELECT 1 FROM messages m
                       WHERE m.session_id = s.id
                         AND m.role = 'assistant'
                         AND (m.tool_calls LIKE '%code_task%'
                              OR m.tool_calls LIKE '%delegate_task%'
                              OR m.tool_calls LIKE '%load_tools%')
                   )",
                params![cutoff],
                |r| r.get(0),
            )?;
            Ok((total, escalating))
        })?;

        Ok(SessionMixReport {
            days,
            total_sessions,
            escalating_sessions,
            escalation_share: safe_div(escalating_sessions as f64, total_sessions as f64),
            by_source,
        })
    }
}

impl Store {
    /// ADR-038 P2 escalation-rate inputs over the last `days` days:
    /// `(light_sessions, escalated_sessions)` — sessions born on the light
    /// profile, and how many of those escalated to full.
    pub fn profile_stats(&self, days: f64) -> Result<(i64, i64), StoreError> {
        let cutoff = now_epoch() - days * 86_400.0;
        self.with_read(|conn| {
            conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(escalated_at IS NOT NULL), 0)
                 FROM sessions WHERE profile = 'light' AND started_at >= ?1",
                params![cutoff],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
    }
}

/// 0.0 for a zero denominator instead of NaN — every ratio in the report must
/// be JSON-safe, and "no data yet" should read as 0, not `null`/NaN.
fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[cfg(test)]
#[path = "tests/session_mix_tests.rs"]
mod tests;
