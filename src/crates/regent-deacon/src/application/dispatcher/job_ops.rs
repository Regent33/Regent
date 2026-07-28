//! `job.list` / `job.cancel` — W9's correctness half, and the one part of it
//! the plan's "comes free with W1" was wrong about.
//!
//! W1 built the whole cancellation path and left both ends unconnected.
//! `JobLedger::request_cancel` sets the flag; `run_to_completion` polls it every
//! 15 seconds and stops the job when it flips. Checked against source
//! 2026-07-29, **`request_cancel` had no caller outside its own test** — so the
//! poll could never fire in production, and a background job the user was told
//! would "report back" ran its full 45-minute deadline with no way to stop it.
//! The mechanism was complete and unreachable.
//!
//! Same for visibility: job state reached the user only by riding
//! `wrap_prompt` into their next turn. There was no way to ask.
//!
//! Cancellation is **cooperative and durable**, and the reply says so rather
//! than implying a kill: the flag is written to the ledger, the runner notices
//! on its next poll, and the job closes as `cancelled` — never as succeeded,
//! because `JobLedger::stop` takes a `StopReason` and not a free state.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, err_response, ok_response};
use serde_json::json;

impl Dispatcher {
    pub(super) fn job_list(&self, req: RpcRequest) {
        let items: Vec<_> = self
            .sessions
            .jobs()
            .live()
            .iter()
            .map(|job| {
                json!({
                    "id": job.id,
                    "kind": job.kind,
                    "label": job.label,
                    "state": job.state,
                    "session_id": job.session_id,
                    "deadline_at": job.deadline_at,
                    "cancel_requested": job.cancel_requested,
                    "created_at": job.created_at,
                })
            })
            .collect();
        self.send(ok_response(req.id, json!(items)));
    }

    pub(super) fn job_cancel(&self, req: RpcRequest) {
        let Some(id) = req.params.get("id").and_then(|v| v.as_str()) else {
            self.send(err_response(req.id, -32602, "job.cancel needs id"));
            return;
        };
        // False means the job is not live — already finished, or never existed.
        // Reported as `requested: false` rather than an error: asking to cancel
        // something that already stopped is not a failure, and the caller wants
        // to know which of the two happened.
        let requested = self.sessions.jobs().request_cancel(id);
        self.send(ok_response(
            req.id,
            json!({
                "requested": requested,
                "note": if requested {
                    "cancellation requested; the job stops at its next check (within 15s)"
                } else {
                    "no live job with that id — it may have already finished"
                },
            }),
        ));
    }
}
