//! The `background_task` tool + its in-memory task board — fire-and-acknowledge
//! for long jobs (building software, deep research, producing documents,
//! spreadsheets, decks). The tool spawns a detached full-toolset agent session
//! and returns immediately, so a live voice/chat turn never blocks on the work.
//! Results come back through `wrap_prompt`: the dispatcher calls it on every
//! real `prompt.submit`, prepending finished results and running-task status to
//! the user's text, and the model relays them naturally in its reply.

use crate::application::session_manager::SessionManager;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_tools::{ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Weak};
use std::time::Instant;

/// A finished result longer than this is trimmed before prompt injection —
/// the full output still lives in the background session's transcript.
const RESULT_MAX_CHARS: usize = 4000;

enum Status {
    Running,
    Done(String),
    Failed(String),
}

struct Task {
    id: u64,
    label: String,
    started: Instant,
    status: Status,
}

// ponytail: process-global board — one deacon process, one board. Move onto
// SessionManager if per-tenant isolation ever matters.
static BOARD: Mutex<Vec<Task>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn start_task(label: String) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    BOARD.lock().unwrap().push(Task {
        id,
        label,
        started: Instant::now(),
        status: Status::Running,
    });
    id
}

fn finish_task(id: u64, outcome: Result<String, String>) {
    let mut board = BOARD.lock().unwrap();
    if let Some(task) = board.iter_mut().find(|t| t.id == id) {
        task.status = match outcome {
            Ok(report) => Status::Done(report),
            Err(error) => Status::Failed(error),
        };
    }
}

/// Prepend background-task updates to a real user turn. Finished tasks are
/// delivered once and dropped (the wrapped prompt lands in session history, so
/// even a barged-over spoken reply keeps the fact recoverable); running tasks
/// are summarized so "how's it going?" answers without a tool call. Returns
/// the text unchanged when the board is empty.
pub fn wrap_prompt(text: &str) -> String {
    let mut board = BOARD.lock().unwrap();
    if board.is_empty() {
        return text.to_owned();
    }
    let mut note = String::new();
    for task in board.iter() {
        match &task.status {
            Status::Running => {
                let mins = task.started.elapsed().as_secs() / 60;
                note.push_str(&format!("- STILL RUNNING ({mins}m): {}\n", task.label));
            }
            Status::Done(report) => {
                let mut r = report.trim().to_owned();
                if r.len() > RESULT_MAX_CHARS {
                    r.truncate(RESULT_MAX_CHARS);
                    r.push_str("… (trimmed)");
                }
                note.push_str(&format!("- FINISHED: {}\n  Result: {r}\n", task.label));
            }
            Status::Failed(error) => {
                note.push_str(&format!("- FAILED: {}\n  Error: {error}\n", task.label));
            }
        }
    }
    board.retain(|t| matches!(t.status, Status::Running));
    format!(
        "[System note — background task update, not yet seen by the user; relay it naturally in \
         your reply (on a call: speak the takeaway in a sentence or two):\n{note}]\n\n{text}"
    )
}

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "background_task".into(),
        description: "Start a long-running job in the background and return IMMEDIATELY: \
             building or changing software, deep research, generating documents, spreadsheets, \
             or presentations — anything needing more than a minute or two of work. A separate \
             agent with your full toolset runs it to completion; its result is delivered to you \
             automatically in a later turn, and you relay it to the user then. After calling \
             this, tell the user the job has started and that you'll report back — do NOT wait \
             or poll. Not for quick lookups or questions you can answer in this turn."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "task": {"type": "string", "description": "The job, in full, with all context the user gave — the background agent sees nothing else."},
                "label": {"type": "string", "description": "A short human label for the job, e.g. 'build the expense tracker'."}
            },
            "required": ["task"]
        }),
        toolset: "delegation".into(),
    }
}

/// Runs the job on a detached session via the live manager. `Weak` so the tool
/// never keeps the manager alive past shutdown.
pub struct BackgroundTaskTool {
    sessions: Weak<SessionManager>,
}

impl BackgroundTaskTool {
    #[must_use]
    pub fn new(sessions: Weak<SessionManager>) -> Self {
        Self { sessions }
    }
}

#[async_trait]
impl ToolExecutor for BackgroundTaskTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(task) = args.get("task").and_then(Value::as_str) else {
            return Ok(tool_error_json("missing required parameter: task"));
        };
        let Some(sessions) = self.sessions.upgrade() else {
            return Ok(tool_error_json("deacon is shutting down"));
        };
        let label = args
            .get("label")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| task.chars().take(60).collect());
        let id = start_task(label.clone());
        let task = task.to_owned();
        tokio::spawn(async move {
            let outcome = sessions
                .run_detached_task(&task)
                .await
                .map_err(|e| e.to_string());
            finish_task(id, outcome);
        });
        Ok(json!({
            "started": true,
            "task_id": id,
            "label": label,
            "note": "Running in the background. Tell the user it's started and you'll report \
                     the result when it's ready — do NOT wait for it in this turn."
        })
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_delivers_finished_once_and_keeps_running() {
        let id_done = start_task("make the deck".into());
        finish_task(id_done, Ok("Deck at artifacts/deck/".into()));
        let _id_running = start_task("build the app".into());

        let first = wrap_prompt("how's it going?");
        assert!(first.contains("FINISHED: make the deck"), "{first}");
        assert!(first.contains("STILL RUNNING"), "{first}");
        assert!(first.ends_with("how's it going?"), "{first}");

        // Finished item was delivered and dropped; running one persists.
        let second = wrap_prompt("and now?");
        assert!(!second.contains("make the deck"), "{second}");
        assert!(second.contains("build the app"), "{second}");
    }

    #[test]
    fn wrap_is_identity_when_board_empty() {
        // Isolated from the other test's state only by label choice: an empty
        // board must return the exact input.
        let before = BOARD.lock().unwrap().is_empty();
        if before {
            assert_eq!(wrap_prompt("plain"), "plain");
        }
    }
}
