//! `reminder` — the everyday "remind me" surface. It does NOT schedule
//! anything itself: it creates/lists/cancels one-shot (or recurring) jobs in
//! the SAME `$REGENT_HOME/cron` store the deacon's scheduler already ticks
//! (`FsJobRepository`, jobs.json), so a reminder actually fires. The job's
//! prompt is `"Reminder: <message>"` — a fresh-context agent surfaces it to
//! the user when the scheduler runs it (the same shape `cron.add` writes).

use super::reminder_time::{daily_to_utc, fmt_local, resolve_at};
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_cron::{CronJob, FsJobRepository, JobRepository, Schedule};
use regent_kernel::{RegentError, ToolDefinition, tool_result_json};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Reminders are the cron jobs with this `CronJob::name` — the typed
/// discriminator `list`/`cancel` scope to, so they can never touch a
/// report/doc-forge job. The prompt additionally starts with [`MARKER`]
/// purely so the fire-time agent surfaces it as a reminder.
const JOB_NAME: &str = "reminder";
const MARKER: &str = "Reminder: ";

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "reminder".into(),
        description: "Set, list, or cancel a personal reminder. `add` needs `message` plus \
                      exactly one of `at` (a one-off time — \"HH:MM\" today/tomorrow, or an ISO \
                      datetime like 2026-07-20T09:00) or `every` (a recurrence — \"30m\", \"2h\", \
                      \"1d\", or \"daily 09:30\" in the user's local time). Give absolute/ISO times when you can — resolve \
                      relative phrases like \"tomorrow 3pm\" to a datetime first. `list` shows \
                      pending reminders; `cancel` needs the reminder `id`."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["add", "list", "cancel"]},
                "message": {"type": "string", "description": "What to be reminded of (add)."},
                "at": {"type": "string", "description": "One-off fire time: \"HH:MM\" or ISO datetime."},
                "every": {"type": "string", "description": "Recurrence: \"30m\", \"2h\", \"1d\", \"daily 09:30\"."},
                "id": {"type": "string", "description": "Reminder id to cancel."}
            },
            "required": ["action"]
        }),
        toolset: "everyday".into(),
    }
}

pub struct ReminderTool;

#[async_trait]
impl ToolExecutor for ReminderTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let repo = open_repo()?;
        self.run(&args, &repo)
    }
}

impl ReminderTool {
    /// Core logic over any `JobRepository` (the deacon's `FsJobRepository` in
    /// prod, a temp-dir one in tests) — the seam that keeps this env-free.
    fn run(&self, args: &Value, repo: &dyn JobRepository) -> Result<String, RegentError> {
        match args.get("action").and_then(Value::as_str) {
            Some("add") => self.add(args, repo),
            Some("list") => list(repo),
            Some("cancel") => cancel(args, repo),
            other => Err(bad(format!(
                "unknown action {other:?} — use \"add\", \"list\", or \"cancel\""
            ))),
        }
    }

    fn add(&self, args: &Value, repo: &dyn JobRepository) -> Result<String, RegentError> {
        let message = args
            .get("message")
            .and_then(Value::as_str)
            .filter(|m| !m.trim().is_empty())
            .ok_or_else(|| bad("`add` requires a non-empty `message`"))?;
        let at = args.get("at").and_then(Value::as_str);
        let every = args.get("every").and_then(Value::as_str);
        let schedule = match (at, every) {
            (Some(at), None) => {
                let at_epoch = resolve_at(at).map_err(bad)?;
                // A past one-shot would be silently dropped by the scheduler's
                // catch-up window — reject it here where the model can react.
                if at_epoch <= regent_store::now_epoch() {
                    return Err(bad(format!(
                        "'{at}' is in the past — reminders need a future time"
                    )));
                }
                Schedule::OneShot { at_epoch }
            }
            // "daily HH:MM" is UTC in regent-cron; users mean local wall
            // time, so convert before parsing.
            (None, Some(every)) => {
                Schedule::parse(&daily_to_utc(every)).map_err(|e| bad(e.to_string()))?
            }
            _ => return Err(bad("`add` needs exactly one of `at` or `every`")),
        };
        let prompt = format!("{MARKER}{}", message.trim());
        let job = CronJob::new(JOB_NAME, schedule, prompt, regent_store::now_epoch())
            .map_err(|e| bad(e.to_string()))?;
        let (id, next) = (job.id.clone(), job.next_run_at);
        repo.mutate(&mut |jobs| jobs.push(job.clone()))
            .map_err(|e| bad(e.to_string()))?;
        Ok(tool_result_json(json!({
            "id": id,
            "message": message.trim(),
            "fires": fmt_local(next),
            "next_run_at": next,
        })))
    }
}

fn list(repo: &dyn JobRepository) -> Result<String, RegentError> {
    let jobs = repo.load().map_err(|e| bad(e.to_string()))?;
    let items: Vec<Value> = jobs
        .iter()
        .filter(|j| j.name == JOB_NAME)
        .map(|j| {
            json!({
                "id": j.id,
                "message": j.prompt.strip_prefix(MARKER).unwrap_or(&j.prompt),
                "next_fire": fmt_local(j.next_run_at),
                "next_run_at": j.next_run_at,
                "enabled": j.enabled,
            })
        })
        .collect();
    Ok(tool_result_json(json!({
        "count": items.len(),
        "reminders": items,
    })))
}

fn cancel(args: &Value, repo: &dyn JobRepository) -> Result<String, RegentError> {
    let id = args
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("`cancel` requires the reminder `id`"))?;
    let mut removed = false;
    repo.mutate(&mut |jobs| {
        let before = jobs.len();
        // Scoped to reminder jobs — an id belonging to a report/doc-forge
        // cron job must never be deletable through this tool.
        jobs.retain(|j| j.id != id || j.name != JOB_NAME);
        removed = jobs.len() < before;
    })
    .map_err(|e| bad(e.to_string()))?;
    Ok(tool_result_json(json!({
        "removed": removed,
        "id": id,
        "detail": if removed { "reminder cancelled" } else { "no reminder with that id" },
    })))
}

/// The cron store the deacon ticks: `$REGENT_HOME/cron` (mirrors
/// `regent-deacon`'s `regent_home()` fallback — `USERPROFILE`/`HOME` + `.regent`).
fn open_repo() -> Result<FsJobRepository, RegentError> {
    let home = std::env::var("REGENT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let user = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(user).join(".regent")
        });
    FsJobRepository::new(home.join("cron")).map_err(|e| bad(e.to_string()))
}

fn bad(message: impl Into<String>) -> RegentError {
    RegentError::Tool {
        tool: "reminder".into(),
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "tests/reminder.rs"]
mod tests;
