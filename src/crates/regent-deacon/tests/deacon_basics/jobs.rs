//! A background job has to announce itself.
//!
//! The reported bug: a job ran to completion, the work was done, and the user
//! sat waiting because the agent had said "I'll report back". It could not.
//! Job state reached the client by exactly one route — `wrap_prompt` folding it
//! into the user's NEXT message — so a finished job stayed silent for as long
//! as the user stayed silent, which is precisely when they were waiting.

use crate::helpers::{ScriptedProvider, make_session_manager};
use regent_deacon::BackgroundTaskTool;
use regent_tools::{DenyAll, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Semaphore;

/// Drain the notification stream until `method` shows up. Bounded, so a missing
/// notification fails the test instead of hanging it.
async fn wait_for(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
    method: &str,
) -> Option<Value> {
    let deadline = tokio::time::Duration::from_secs(10);
    tokio::time::timeout(deadline, async {
        while let Some(line) = rx.recv().await {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.get("method").and_then(Value::as_str) == Some(method) {
                return Some(value);
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

#[tokio::test]
async fn a_finished_job_announces_itself_without_waiting_for_the_user_to_speak() {
    let dir = TempDir::new().unwrap();
    // One reply is enough: the detached agent answers and its turn ends.
    let provider = ScriptedProvider::with(vec![ScriptedProvider::text_reply("served it")]);
    let (sm, mut rx) = make_session_manager(&dir, provider);

    let tool = BackgroundTaskTool::new(
        Arc::downgrade(&sm),
        sm.jobs(),
        Arc::new(Semaphore::new(1)),
    );
    let ctx = ToolContext::new(dir.path().to_path_buf(), Arc::new(DenyAll));
    let started = tool
        .execute(
            json!({ "task": "serve the site", "label": "serve the site" }),
            &ctx,
        )
        .await
        .unwrap();
    let started: Value = serde_json::from_str(&started).unwrap();
    assert_eq!(started["started"], true, "{started}");

    // The whole point: this arrives with no further user input.
    let event = wait_for(&mut rx, "job.finished")
        .await
        .expect("a finished job must announce itself; the user is not going to ask");
    let params = &event["params"];
    assert_eq!(params["label"], "serve the site", "{params}");
    assert_eq!(params["job_id"], started["job_id"], "{params}");
    // The state is reported as-is. A job that failed or timed out must never
    // reach the client wearing "finished" — the same discipline `wrap_prompt`
    // enforces for the text note.
    let state = params["state"].as_str().unwrap_or_default();
    assert!(
        ["finished", "failed", "timed_out", "cancelled"].contains(&state),
        "unexpected terminal state {state}"
    );
}
