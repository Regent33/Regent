//! `schedule_task` behavior: jobs are stamped with the creating surface so
//! the right scheduler runs them, and `list` never leaks another chat's jobs.

use super::*;
use regent_cron::{CronError, TickGuard};
use std::sync::Mutex;

/// In-memory job store — the tool's contract is the repository, not the disk.
struct MemJobs(Mutex<Vec<CronJob>>);

impl JobRepository for MemJobs {
    fn load(&self) -> Result<Vec<CronJob>, CronError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, jobs: &[CronJob]) -> Result<(), CronError> {
        *self.0.lock().unwrap() = jobs.to_vec();
        Ok(())
    }
    fn try_lock_tick(&self) -> Result<Option<TickGuard>, CronError> {
        Ok(None)
    }
}

fn store() -> MemJobs {
    MemJobs(Mutex::new(Vec::new()))
}

#[test]
fn added_jobs_carry_the_calling_surface_and_a_local_next_run() {
    let jobs = store();
    let out = run_action(
        &json!({"action": "add", "name": "evening", "schedule": "daily 8pm",
                "prompt": "summarise the day"}),
        &jobs,
        Some("telegram:42"),
    );
    assert!(out.contains("\"success\":true"), "{out}");
    // Rendered for a human, not as a raw epoch — a wrong timezone must be
    // visible in the reply rather than silently firing hours off.
    assert!(out.contains("08:00 PM"), "local wall clock in reply: {out}");

    let saved = jobs.load().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].target.as_deref(), Some("telegram:42"));
}

#[test]
fn list_shows_only_this_conversations_jobs() {
    let jobs = store();
    for target in ["telegram:42", "telegram:99"] {
        run_action(
            &json!({"action": "add", "name": "j", "schedule": "1h", "prompt": "p"}),
            &jobs,
            Some(target),
        );
    }
    run_action(
        &json!({"action": "add", "name": "local", "schedule": "1h", "prompt": "p"}),
        &jobs,
        None,
    );

    let mine: Vec<Value> = serde_json::from_str(&list(&jobs, Some("telegram:42"))).unwrap();
    assert_eq!(mine.len(), 1, "other chats' jobs must not leak: {mine:?}");
}

#[test]
fn a_bad_schedule_explains_the_grammar_instead_of_failing_silently() {
    let out = run_action(
        &json!({"action": "add", "name": "j", "schedule": "every monday", "prompt": "p"}),
        &store(),
        None,
    );
    assert!(out.contains("daily 8pm"), "should teach the format: {out}");
}

#[test]
fn remove_reports_an_unknown_id() {
    let jobs = store();
    let added = run_action(
        &json!({"action": "add", "name": "j", "schedule": "1h", "prompt": "p"}),
        &jobs,
        None,
    );
    let id = serde_json::from_str::<Value>(&added).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    assert!(run_action(&json!({"action": "remove", "id": "nope"}), &jobs, None).contains("error"));
    assert!(
        run_action(&json!({"action": "remove", "id": id}), &jobs, None)
            .contains("\"success\":true")
    );
    assert!(jobs.load().unwrap().is_empty());
}
