//! `schedule_task` — the agent's own prospective memory: "remind me at 8pm",
//! "every morning, summarise my inbox". Writes the SAME `$REGENT_HOME/cron`
//! job store the deacon and CLI use, so a job scheduled from chat shows up in
//! `regent cron list` and vice versa.
//!
//! Jobs are stamped with the creating surface's `target` (see
//! `CronJob::target`) so the scheduler that can actually answer the user is
//! the one that runs them. A surface with no delivery channel passes `None`.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use chrono::TimeZone;
use regent_cron::{CronJob, JobRepository, Schedule};
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::sync::Arc;

/// Registers `schedule_task` against a job store. `target` binds new jobs to
/// this surface (`"telegram:12345"`); `None` leaves them to the local deacon.
pub fn register_schedule_tool(
    catalog: &mut ToolCatalog,
    jobs: Arc<dyn JobRepository>,
    target: Option<String>,
) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(ScheduleTaskTool { jobs, target }))
}

fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "schedule_task".into(),
        description: "Schedule your OWN work to run later, unattended — reminders, recurring \
                      reports, 'do X at 8pm'. The job runs as a fresh you with the `prompt` as \
                      its instruction, and the result is delivered back to this conversation. \
                      Use this whenever the user asks for something at a time or on a repeat; \
                      never promise to 'remember' a time yourself, you are not running then. \
                      `schedule` formats (ALL WALL CLOCK TIMES ARE THE USER'S LOCAL TIME): \
                      '30m' / '2h' / '1d' = every N; 'daily 07:30' / 'daily 8pm' = once a day; \
                      'once 8pm' / 'once 07:30' = the next time that clock reads it, then the \
                      job retires. Write the prompt as a full standalone instruction — the run \
                      has NO memory of this conversation. action 'list' shows scheduled jobs, \
                      'remove' cancels one by id."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["add", "list", "remove"]},
                "name": {"type": "string", "description": "Short label, e.g. 'evening summary'."},
                "schedule": {"type": "string", "description": "'30m' | 'daily 8pm' | 'once 19:45' (local time)."},
                "prompt": {"type": "string", "description": "Standalone instruction for the scheduled run."},
                "id": {"type": "string", "description": "Job id (for 'remove')."}
            },
            "required": ["action"]
        }),
        toolset: "schedule".into(),
    }
}

struct ScheduleTaskTool {
    jobs: Arc<dyn JobRepository>,
    target: Option<String>,
}

#[async_trait]
impl ToolExecutor for ScheduleTaskTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let jobs = Arc::clone(&self.jobs);
        let target = self.target.clone();
        // The store is blocking file I/O behind a lock; keep it off the runtime.
        tokio::task::spawn_blocking(move || run_action(&args, jobs.as_ref(), target.as_deref()))
            .await
            .map_err(|e| RegentError::Tool {
                tool: "schedule_task".into(),
                message: e.to_string(),
            })
    }
}

fn run_action(args: &Value, jobs: &dyn JobRepository, target: Option<&str>) -> String {
    match args.get("action").and_then(Value::as_str) {
        Some("add") => add(args, jobs, target),
        Some("list") => list(jobs, target),
        Some("remove") => remove(args, jobs),
        _ => tool_error_json("action must be one of: add, list, remove"),
    }
}

fn add(args: &Value, jobs: &dyn JobRepository, target: Option<&str>) -> String {
    let field = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let (Some(name), Some(raw), Some(prompt)) = (field("name"), field("schedule"), field("prompt"))
    else {
        return tool_error_json("add needs name, schedule and prompt");
    };
    let schedule = match Schedule::parse(raw) {
        Ok(s) => s,
        Err(e) => {
            return tool_error_json(format!(
                "{e} — use '30m' / '2h' / '1d', 'daily 8pm', or 'once 19:45'"
            ));
        }
    };
    let job = match CronJob::new(name, schedule, prompt, now_epoch()) {
        Ok(j) => match target {
            Some(t) => j.for_target(t),
            None => j,
        },
        Err(e) => return tool_error_json(e.to_string()),
    };
    let (id, next) = (job.id.clone(), job.next_run_at);
    match jobs.mutate(&mut |all| all.push(job.clone())) {
        Ok(()) => json!({
            "success": true, "id": id, "name": name,
            "next_run": local_time(next),
        })
        .to_string(),
        Err(e) => tool_error_json(e.to_string()),
    }
}

fn list(jobs: &dyn JobRepository, target: Option<&str>) -> String {
    match jobs.load() {
        // Only this conversation's jobs: another chat's reminders are not
        // this user's business, and the deacon's own jobs aren't actionable
        // from here.
        Ok(all) => json!(
            all.iter()
                .filter(|j| j.target.as_deref() == target)
                .map(|j| json!({
                    "id": j.id, "name": j.name, "prompt": j.prompt,
                    "enabled": j.enabled, "next_run": local_time(j.next_run_at),
                }))
                .collect::<Vec<_>>()
        )
        .to_string(),
        Err(e) => tool_error_json(e.to_string()),
    }
}

fn remove(args: &Value, jobs: &dyn JobRepository) -> String {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return tool_error_json("remove needs id (see action 'list')");
    };
    let mut found = false;
    let result = jobs.mutate(&mut |all| {
        let before = all.len();
        all.retain(|j| j.id != id);
        found = all.len() < before;
    });
    match result {
        Err(e) => tool_error_json(e.to_string()),
        Ok(()) if found => json!({"success": true, "removed": id}).to_string(),
        Ok(()) => tool_error_json(format!("no scheduled job with id {id}")),
    }
}

fn now_epoch() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

/// Epoch → the user's local wall clock. Jobs are stored in epoch seconds, but
/// echoing that back is unreadable and hides an hours-off bug.
fn local_time(epoch: f64) -> String {
    chrono::Local
        .timestamp_opt(epoch as i64, 0)
        .single()
        .map_or_else(
            || epoch.to_string(),
            |t| t.format("%a %d %b %Y, %I:%M %p").to_string(),
        )
}

#[cfg(test)]
#[path = "tests/schedule_task.rs"]
mod tests;
