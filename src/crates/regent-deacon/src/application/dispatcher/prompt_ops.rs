//! The streamed `prompt.submit` turn: delta notifications, queueing, the
//! post-turn telemetry, and the raw-error → actionable-sentence mapping.
//! Split from `session_ops.rs` (file-size rule).

use super::Dispatcher;
use crate::application::session_manager::SessionManager;
use crate::domain::entities::{RpcNotification, RpcRequest, err_response, ok_response};
use crate::domain::errors::DeaconError;
use regent_kernel::{RegentError, SessionId};
use serde_json::json;
use std::sync::Arc;

impl Dispatcher {
    pub(super) fn prompt_submit(&self, req: RpcRequest) {
        let id = req.id.clone();
        let Some(sid_str) = req
            .params
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
        else {
            self.send(err_response(req.id, -32602, "missing session_id"));
            return;
        };
        let Some(mut text) = req
            .params
            .get("text")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
        else {
            self.send(err_response(req.id, -32602, "missing text"));
            return;
        };
        let turn_provider = match with_model_prompt(&text) {
            Ok(Some((model, prompt))) => {
                let Some(cfg) = self.config_snapshot() else {
                    self.send(err_response(
                        req.id,
                        -32602,
                        "/with requires a configured providers map",
                    ));
                    return;
                };
                if super::providers_ops::split_provider_model(&cfg, &model).is_none() {
                    self.send(err_response(
                        req.id,
                        -32602,
                        format!(
                            "unknown model route '{model}' — use /with <configured-provider>/<model> <task>"
                        ),
                    ));
                    return;
                }
                let provider = match super::providers_ops::explicit_provider(&cfg, &model) {
                    Ok(provider) => provider,
                    Err(error) => {
                        self.send(err_response(
                            req.id,
                            -32000,
                            format!("cannot use explicit model route '{model}': {error}"),
                        ));
                        return;
                    }
                };
                text = prompt;
                Some(provider)
            }
            Ok(None) => None,
            Err(message) => {
                self.send(err_response(req.id, -32602, message));
                return;
            }
        };
        // The raw opening message drives first-turn title generation — captured
        // before we decorate the prompt with attachment refs / job wrapping.
        let title_source = text.clone();
        // `/learn` (advertised in commands.list, previously implemented
        // nowhere): rewrite the message into the skill-authoring prompt. The
        // live session does the work with its own tools — every surface that
        // submits prompts gets the command for free.
        if let Some(rest) = text
            .trim_start()
            .strip_prefix("/learn")
            .filter(|r| r.is_empty() || r.starts_with(char::is_whitespace))
        {
            text = learn_prompt(rest.trim());
        }
        let session_id = SessionId::from_string(sid_str.clone());

        // Optional staged attachments (M8): append one ref line per path so the
        // agent's file tools can open it. Only paths under
        // `$REGENT_HOME/attachments` are honored — anything else is rejected so a
        // client can't smuggle an arbitrary filesystem path into the prompt.
        if let Some(items) = req.params.get("attachments").and_then(|v| v.as_array()) {
            let root = super::attachment_ops::attachments_root();
            for item in items {
                let Some(p) = item.as_str() else { continue };
                if !super::attachment_ops::attachment_within_root(&root, std::path::Path::new(p)) {
                    self.send(err_response(
                        req.id,
                        -32602,
                        format!("attachment path is outside the attachments root: {p}"),
                    ));
                    return;
                }
                text.push_str(&format!("\n\n[attached file: {p}]"));
            }
        }

        // What the coding panel has open (and any highlighted lines) rides
        // with the turn, so the agent works on the file the user is looking at
        // instead of asking which one they meant.
        if let Some(note) = super::editor_context::editor_note(&req.params) {
            text.push_str(&note);
        }

        // Decide up-front whether this turn should title the session: only an
        // untitled session whose first user turn is about to run (checked before
        // `run_turn` appends the user message). Cheap store reads.
        let should_title = SessionManager::should_generate_title(
            self.sessions.session_has_title(&session_id),
            self.sessions.prior_user_turns(&session_id),
        );

        // W3 step 2: record what retrieval WOULD have injected for this turn.
        // Before `wrap_prompt`, so the query is the user's actual words rather
        // than the words plus a job-status preamble. Spawned and side-effect
        // free — the turn neither waits for it nor is changed by it.
        crate::application::memory_shadow::record_would_inject(self.sessions.graph(), &text);

        // Deliver background-task results/status with the user's turn — only
        // real client turns pass through here, never detached job sessions.
        // `pending` is confirmed on the SUCCESS arm below: a turn that never
        // ran must not consume the only copy of a job's report.
        let (text, pending_jobs) =
            crate::application::background_task_tool::wrap_prompt(&self.sessions.jobs(), &text);
        self.notify("turn.started", json!({"session_id": sid_str}));

        let sessions = Arc::clone(&self.sessions);
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            let send = |payload: String| {
                out_tx.send(payload).ok();
            };
            let notify = |method: &str, params: serde_json::Value| {
                if let Ok(line) = serde_json::to_string(&RpcNotification::new(method, params)) {
                    out_tx.send(line).ok();
                }
            };
            match sessions
                .run_turn_with_provider(&session_id, &text, turn_provider)
                .await
            {
                Ok(reply) => {
                    // The turn ran and the user has the reply, so any job report
                    // it carried has now actually been delivered. Only here —
                    // the error/interrupt arm deliberately leaves them pending
                    // so the next turn repeats the news instead of losing it.
                    if !pending_jobs.is_empty() {
                        sessions.jobs().mark_delivered(&pending_jobs);
                    }
                    notify(
                        "message.complete",
                        json!({"session_id": session_id.to_string(), "reply": reply}),
                    );
                    // Additive (desktop status-bar ctx meter): the just-finished
                    // turn's token spend + context budget. The desktop populates
                    // the meter only when ALL THREE are present, so attach them
                    // to the SUCCESS turn.complete. Best-effort: an unknown
                    // session simply omits them (payload stays back-compatible).
                    let mut complete = json!({"session_id": session_id.to_string()});
                    if let Some((
                        input_tokens,
                        output_tokens,
                        context_max,
                        cache_read,
                        cache_write,
                        usage_complete,
                        last_request_input_tokens,
                    )) = sessions.last_turn_usage(&session_id).await
                        && let Some(obj) = complete.as_object_mut()
                    {
                        obj.insert("input_tokens".into(), json!(input_tokens));
                        obj.insert("output_tokens".into(), json!(output_tokens));
                        obj.insert("context_max".into(), json!(context_max));
                        obj.insert("usage_complete".into(), json!(usage_complete));
                        if let Some(last_input) = last_request_input_tokens {
                            obj.insert("last_request_input_tokens".into(), json!(last_input));
                        }
                        // Additive (SPL §3.3): the cached/fresh split, present
                        // only when the provider reported prompt-cache usage.
                        if let Some(read) = cache_read {
                            obj.insert("cache_read_tokens".into(), json!(read));
                        }
                        if let Some(write) = cache_write {
                            obj.insert("cache_write_tokens".into(), json!(write));
                        }
                    }
                    // Additive (SPL §3.1): why this turn was full-price, when
                    // known — omitted entirely when the prefix carried over.
                    if let Some(reason) = sessions.last_turn_cache_reset(&session_id).await
                        && let Some(obj) = complete.as_object_mut()
                    {
                        obj.insert("cache_reset".into(), json!(reason));
                    }
                    // Additive (SPL §3.3): build-time stable-prefix tier hashes
                    // so clients can watch Tier 0/1 stability across turns. The
                    // call also runs the fail-open cache_bust check. Best-effort
                    // like the usage fields — omitted for an unknown session.
                    if let Some((tier0_hash, tier1_hash)) =
                        sessions.turn_prefix_hashes(&session_id).await
                        && let Some(obj) = complete.as_object_mut()
                    {
                        obj.insert("tier0_hash".into(), json!(tier0_hash));
                        obj.insert("tier1_hash".into(), json!(tier1_hash));
                    }
                    notify("turn.complete", complete);
                    // First-turn title generation (M8): a cheap aux model call
                    // names the session, then emits `session.titled` so the rail
                    // updates live. Detached so it never delays the reply, and
                    // best-effort so a failure only warns. Titled from the whole
                    // opening EXCHANGE: call sessions open with a bare "hey
                    // boss" — only the reply carries the topic.
                    if should_title {
                        let sessions = Arc::clone(&sessions);
                        let sid = session_id.clone();
                        let source = crate::application::session_manager::exchange_snippet(
                            &title_source,
                            &reply,
                        );
                        tokio::spawn(async move {
                            sessions.generate_title(sid, source).await;
                        });
                    }
                    let resp = ok_response(
                        id,
                        json!({"reply": reply, "session_id": session_id.to_string()}),
                    );
                    if let Ok(line) = serde_json::to_string(&resp) {
                        send(line);
                    }
                }
                Err(error) => {
                    let interrupted = matches!(&error, DeaconError::Core(RegentError::Interrupted));
                    // Interruptions are internal control flow; every other turn
                    // failure is shown/spoken to the user, so make it a clear,
                    // actionable sentence instead of a raw provider dump.
                    let message = if interrupted {
                        error.to_string()
                    } else {
                        humanize_turn_error(&error.to_string())
                    };
                    notify(
                        if interrupted {
                            "turn.interrupted"
                        } else {
                            "turn.complete"
                        },
                        json!({"session_id": session_id.to_string(), "error": message}),
                    );
                    let resp = err_response(id, -32000, message);
                    if let Ok(line) = serde_json::to_string(&resp) {
                        send(line);
                    }
                }
            }
        });
    }
}

use turn_errors::humanize_turn_error;

mod turn_errors;

fn with_model_prompt(text: &str) -> Result<Option<(String, String)>, String> {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed
        .strip_prefix("/with")
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    else {
        return Ok(None);
    };
    let rest = rest.trim_start();
    let Some((model, prompt)) = rest.split_once(char::is_whitespace) else {
        return Err("usage: /with <provider>/<model> <task>".to_owned());
    };
    let prompt = prompt.trim();
    if !model.contains('/') || prompt.is_empty() {
        return Err("usage: /with <provider>/<model> <task>".to_owned());
    }
    Ok(Some((model.to_owned(), prompt.to_owned())))
}

/// The `/learn` prompt: one instruction block that turns whatever the user
/// described — a directory, a URL, this very conversation, pasted notes —
/// into a durable skill via `skill_manage`. The naming/description standards
/// are ENFORCED by the tool itself (`regent_skills::library` rejects
/// violations with actionable errors), so the prompt teaches the shape and
/// the boundary guarantees it — a sloppy model cannot ship a malformed skill.
fn learn_prompt(topic: &str) -> String {
    let source = if topic.is_empty() {
        "what we did together in THIS conversation (read it back — the workflow, \
         corrections, and fixes above are the source material)"
            .to_owned()
    } else {
        format!("the following, as the user described it: {topic}")
    };
    format!(
        "Learn a durable skill from {source}.\n\n\
         1. GATHER with the tools you have: read_file/search_files for paths, \
         web_fetch for URLs, session history for \"what we just did\", the text \
         itself for pasted notes. If skill tools aren't loaded, load_tools first.\n\
         2. Check skills_list FIRST — patch an existing skill (skill_manage \
         action 'patch') when one covers this ground; create is the last \
         resort and must be class-level (\"debug flaky CI\", never \
         \"fix Tuesday's pipeline\").\n\
         3. AUTHOR via skill_manage. Standards (the tool REJECTS violations — \
         on an error, fix exactly what it names and retry):\n\
         - name: lowercase-hyphenated, no spaces.\n\
         - description: ONE sentence, ≤60 chars, ends with a period; states \
         the capability, never the implementation; no marketing words \
         (powerful, comprehensive, seamless, robust).\n\
         - body, in order: when to use (concrete trigger phrases) → \
         prerequisites (exact env vars, credentials) → procedure (numbered, \
         copy-paste-exact commands framed through YOUR tools: say read_file \
         not cat, search_files not grep, terminal for scripts) → pitfalls \
         (limits, things that look broken but aren't) → one verification \
         check that proves the skill worked.\n\
         - never write host-derived identity (usernames, paths with the \
         user's name) into a skill — skills get shared.\n\
         4. Reply with the skill name and one line on what it now covers. If \
         there is genuinely nothing durable to learn, say so and save nothing."
    )
}

#[cfg(test)]
mod with_model_tests {
    use super::with_model_prompt;

    #[test]
    fn parses_a_one_turn_model_route_without_rewriting_plain_chat() {
        assert_eq!(
            with_model_prompt("/with nvidia/nvidia/nemotron review this").unwrap(),
            Some((
                "nvidia/nvidia/nemotron".to_owned(),
                "review this".to_owned()
            ))
        );
        assert_eq!(with_model_prompt("ordinary chat").unwrap(), None);
        assert!(with_model_prompt("/with nvidia/model").is_err());
    }
}
