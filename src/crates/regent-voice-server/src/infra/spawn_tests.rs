use super::*;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn candidate_loop_skips_a_failed_pin_and_uses_the_next_binary() {
    let stale = PathBuf::from("stale-deacon");
    let fresh = PathBuf::from("fresh-deacon");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_probe = Arc::clone(&seen);

    let doomed = stale.clone();
    let selected = first_healthy(&[stale.clone(), fresh.clone()], move |path| {
        seen_probe.lock().unwrap().push(path.clone());
        let doomed = doomed.clone();
        async move {
            if path == doomed {
                Err("health timed out".to_owned())
            } else {
                Ok(path)
            }
        }
    })
    .await
    .expect("fresh candidate should win");

    assert_eq!(selected, fresh);
    assert_eq!(*seen.lock().unwrap(), vec![stale, fresh]);
}

#[tokio::test]
async fn candidate_loop_reports_every_failure() {
    let error = first_healthy::<(), _, _>(
        &[PathBuf::from("old"), PathBuf::from("new")],
        |path| async move { Err(format!("{} failed", path.display())) },
    )
    .await
    .expect_err("all candidates should fail");

    assert!(error.contains("old: old failed"));
    assert!(error.contains("new: new failed"));
}

#[tokio::test]
async fn candidate_loop_stops_at_the_winner_and_never_revisits() {
    let probed = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&probed);
    let candidates = ["first", "second", "third"].map(PathBuf::from);

    let selected = first_healthy(&candidates, move |path| {
        seen.lock().unwrap().push(path.clone());
        async move { Ok(path) }
    })
    .await
    .expect("the first candidate answers");

    assert_eq!(selected, PathBuf::from("first"));
    assert_eq!(*probed.lock().unwrap(), vec![PathBuf::from("first")]);
}

/// /health shows this string, so a long PATH of broken binaries must not turn it
/// into an unreadable wall — the reasons are capped in count and length.
#[tokio::test]
async fn aggregate_failure_text_stays_bounded_and_actionable() {
    let candidates: Vec<PathBuf> = (0..10).map(|i| PathBuf::from(format!("cand{i}"))).collect();

    let error = first_healthy::<(), _, _>(&candidates, |_| async move {
        Err("x".repeat(MAX_REASON_CHARS * 3))
    })
    .await
    .expect_err("all candidates fail");

    assert!(error.contains("no working regent-deacon among 10 candidate(s)"));
    assert!(error.contains("cand0"));
    assert!(
        error.contains(&format!("(+{} more)", 10 - MAX_REPORTED_FAILURES)),
        "hidden failures must still be counted: {error}"
    );
    assert!(!error.contains("cand9"), "beyond the cap: {error}");
    assert!(!error.contains('\n'), "single line: {error}");
    let ceiling = 120 + MAX_REPORTED_FAILURES * (MAX_REASON_CHARS + 40);
    assert!(error.len() <= ceiling, "{} chars", error.len());
}

/// A losing candidate must be killed AND reaped before the next is tried: a
/// probe that returns while the child still runs leaves an orphan deacon
/// holding the same session store.
#[tokio::test]
async fn discarding_a_losing_candidate_kills_and_reaps_it() {
    let mut cmd = if cfg!(windows) {
        let mut cmd = Command::new("ping");
        cmd.args(["-n", "60", "127.0.0.1"]);
        cmd
    } else {
        let mut cmd = Command::new("sleep");
        cmd.arg("60");
        cmd
    };
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let child = cmd.spawn().expect("spawn a long-lived stand-in child");
    assert!(child.id().is_some(), "the stand-in child should be running");

    // Left alone it would run for a minute; `discard` awaiting wait() to
    // completion is the OS-level proof that it was killed and reaped.
    tokio::time::timeout(Duration::from_secs(10), discard(child))
        .await
        .expect("discard must kill and reap, not wait out the child");
}

#[tokio::test]
async fn a_candidate_that_cannot_launch_reports_why_without_leaking() {
    let dir = tempfile::tempdir().unwrap();
    let bogus = dir.path().join(crate::infra::locate::deacon_name());
    std::fs::write(&bogus, b"not a real binary").unwrap();

    let reason = match probe_candidate(&bogus).await {
        Ok(_) => panic!("a text file must not pass as a deacon"),
        Err(reason) => reason,
    };
    assert!(reason.starts_with("spawn failed:"), "{reason}");
}
