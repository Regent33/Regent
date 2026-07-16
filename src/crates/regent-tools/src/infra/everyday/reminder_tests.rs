//! Reminder tool over a temp-dir cron store. Uses the `run(&self, args, repo)`
//! seam so no global `REGENT_HOME` env is touched — the executor's `execute`
//! just wraps `run` with `open_repo()`.

use super::*;
use regent_cron::FsJobRepository;

fn temp_repo() -> (tempfile::TempDir, FsJobRepository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsJobRepository::new(dir.path().join("cron")).unwrap();
    (dir, repo)
}

fn parse(out: &str) -> Value {
    serde_json::from_str(out).unwrap()
}

#[test]
fn add_at_writes_a_wellformed_oneshot_job_the_repo_reads_back() {
    let (_dir, repo) = temp_repo();
    let out = ReminderTool
        .run(
            &json!({"action": "add", "message": "call the dentist", "at": "2999-01-01T09:00:00"}),
            &repo,
        )
        .unwrap();
    let v = parse(&out);
    assert!(v["id"].as_str().unwrap().starts_with("job_"), "{v}");
    assert!(v["fires"].as_str().unwrap().contains("2999"), "{v}");

    // The scheduler reads it back as a real CronJob with the fire-shaped prompt.
    let jobs = repo.load().unwrap();
    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    assert_eq!(job.prompt, "Reminder: call the dentist");
    assert!(matches!(job.schedule, Schedule::OneShot { .. }), "{:?}", job.schedule);
    assert!(job.enabled);
    assert_eq!(job.id, v["id"].as_str().unwrap());
}

#[test]
fn add_every_writes_a_recurring_job() {
    let (_dir, repo) = temp_repo();
    ReminderTool
        .run(
            &json!({"action": "add", "message": "stand up", "every": "30m"}),
            &repo,
        )
        .unwrap();
    let jobs = repo.load().unwrap();
    assert_eq!(jobs[0].schedule, Schedule::Every { seconds: 1_800 });
    assert_eq!(jobs[0].prompt, "Reminder: stand up");
}

#[test]
fn list_shows_pending_reminders_only() {
    let (_dir, repo) = temp_repo();
    // A non-reminder cron job must NOT leak into the reminder list.
    repo.mutate(&mut |jobs| {
        jobs.push(
            CronJob::new(
                "report",
                Schedule::Every { seconds: 3_600 },
                "Generate the daily report",
                regent_store::now_epoch(),
            )
            .unwrap(),
        );
    })
    .unwrap();
    ReminderTool
        .run(
            &json!({"action": "add", "message": "water plants", "every": "1d"}),
            &repo,
        )
        .unwrap();

    let v = parse(&ReminderTool.run(&json!({"action": "list"}), &repo).unwrap());
    assert_eq!(v["count"], 1, "only the reminder, not the report: {v}");
    assert_eq!(v["reminders"][0]["message"], "water plants");
    assert!(!v["reminders"][0]["next_fire"].as_str().unwrap().is_empty());
}

#[test]
fn cancel_removes_the_reminder() {
    let (_dir, repo) = temp_repo();
    let id = parse(
        &ReminderTool
            .run(
                &json!({"action": "add", "message": "pay rent", "every": "1d"}),
                &repo,
            )
            .unwrap(),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let v = parse(&ReminderTool.run(&json!({"action": "cancel", "id": id}), &repo).unwrap());
    assert_eq!(v["removed"], true, "{v}");
    assert!(repo.load().unwrap().is_empty());

    // Cancelling an unknown id is a clean no-op, not an error.
    let v = parse(&ReminderTool.run(&json!({"action": "cancel", "id": "job_nope"}), &repo).unwrap());
    assert_eq!(v["removed"], false, "{v}");
}

#[test]
fn cancel_never_touches_non_reminder_cron_jobs() {
    let (_dir, repo) = temp_repo();
    repo.mutate(&mut |jobs| {
        jobs.push(
            CronJob::new(
                "report",
                Schedule::Every { seconds: 3_600 },
                "Generate the daily report",
                regent_store::now_epoch(),
            )
            .unwrap(),
        );
    })
    .unwrap();
    let report_id = repo.load().unwrap()[0].id.clone();

    // A reminder cancel aimed at a report job's id must be a no-op.
    let v = parse(
        &ReminderTool
            .run(&json!({"action": "cancel", "id": report_id}), &repo)
            .unwrap(),
    );
    assert_eq!(v["removed"], false, "{v}");
    assert_eq!(repo.load().unwrap().len(), 1, "report job survives");
}

#[test]
fn bad_args_error_clearly() {
    let (_dir, repo) = temp_repo();

    // Missing message.
    let e = ReminderTool
        .run(&json!({"action": "add", "at": "09:00"}), &repo)
        .unwrap_err()
        .to_string();
    assert!(e.contains("message"), "{e}");

    // Both at and every.
    let e = ReminderTool
        .run(
            &json!({"action": "add", "message": "x", "at": "09:00", "every": "1d"}),
            &repo,
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("exactly one"), "{e}");

    // Neither at nor every.
    let e = ReminderTool
        .run(&json!({"action": "add", "message": "x"}), &repo)
        .unwrap_err()
        .to_string();
    assert!(e.contains("exactly one"), "{e}");

    // A past one-shot is rejected, not silently scheduled.
    let e = ReminderTool
        .run(
            &json!({"action": "add", "message": "x", "at": "2001-01-01T09:00:00"}),
            &repo,
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("in the past"), "{e}");

    // Unparseable time.
    let e = ReminderTool
        .run(&json!({"action": "add", "message": "x", "at": "someday"}), &repo)
        .unwrap_err()
        .to_string();
    assert!(e.contains("could not parse"), "{e}");

    // Unknown action.
    let e = ReminderTool
        .run(&json!({"action": "frobnicate"}), &repo)
        .unwrap_err()
        .to_string();
    assert!(e.contains("unknown action"), "{e}");
}

#[test]
fn hhmm_resolves_to_a_future_epoch() {
    // Bare HH:MM is always in the future (today or tomorrow), so CronJob::new
    // never rejects it as a past one-shot.
    let (_dir, repo) = temp_repo();
    ReminderTool
        .run(
            &json!({"action": "add", "message": "lunch", "at": "12:30"}),
            &repo,
        )
        .unwrap();
    let job = &repo.load().unwrap()[0];
    assert!(job.next_run_at > regent_store::now_epoch(), "must fire in the future");
}
