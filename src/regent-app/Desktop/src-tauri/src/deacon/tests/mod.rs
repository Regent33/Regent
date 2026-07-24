//! Deacon-bridge tests. The candidate/respawn logic is driven with a real
//! scratch-home deacon (skipped when the binary is absent so a fresh checkout
//! stays green) and a deliberately bogus first candidate — no source-string
//! assertions, no process-global env mutation (the spawn core is candidate/home
//! injected, so tests never touch REGENT_HOME/REGENT_DEACON_PATH concurrently).

mod respawn;

use super::spawn;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

/// Repo `target/{release,debug}/regent-deacon`, relative to this crate (src-tauri
/// is four levels below the repo root: Desktop → regent-app → src → repo).
pub(super) fn repo_deacon() -> Option<PathBuf> {
    let name = super::locate::deacon_name();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("..");
    for profile in ["release", "debug"] {
        let p = repo.join("target").join(profile).join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

pub(super) fn scratch_home(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("regent-desktop-test-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&home).expect("create scratch REGENT_HOME");
    home
}

#[tokio::test]
async fn health_round_trips_against_real_deacon() {
    let Some(bin) = repo_deacon() else {
        eprintln!("skipping: no built regent-deacon");
        return;
    };
    let home = scratch_home("health");
    let (rpc, mut child) = spawn::spawn_with(vec![bin], home.clone(), |_line| {})
        .await
        .expect("spawn real deacon");

    // (id is not asserted: spawn_with health-probes first, so this is id 2.)
    let resp = rpc
        .request("status.get", json!({}))
        .await
        .expect("status.get returned an error");
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "expected a JSON-RPC result/error, got {resp}"
    );

    rpc.close_stdin().await;
    if tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .is_err()
    {
        child.kill().await.ok();
    }
    std::fs::remove_dir_all(&home).ok();
}

#[tokio::test]
async fn falls_through_a_bad_first_candidate() {
    let Some(bin) = repo_deacon() else {
        eprintln!("skipping: no built regent-deacon");
        return;
    };
    let home = scratch_home("fallback");
    // A real file that is NOT a runnable binary: spawn() fails (Windows: not a
    // valid Win32 app; POSIX: ENOEXEC/EACCES), the loop records it and falls
    // through to the healthy candidate.
    let bogus = home.join("bogus-deacon.exe");
    std::fs::write(&bogus, b"not a real binary").expect("write bogus");

    let (rpc, mut child) = spawn::spawn_with(vec![bogus, bin], home.clone(), |_line| {})
        .await
        .expect("fallback to the healthy candidate");
    let resp = rpc.request("status.get", json!({})).await.expect("status");
    assert!(resp.get("result").is_some() || resp.get("error").is_some());

    rpc.close_stdin().await;
    if tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .is_err()
    {
        child.kill().await.ok();
    }
    std::fs::remove_dir_all(&home).ok();
}

#[tokio::test]
async fn no_candidates_reports_a_build_hint() {
    let home = scratch_home("empty");
    // The Ok type (Arc<DeaconRpc>, Child) isn't Debug, so match rather than
    // expect_err.
    match spawn::spawn_with(vec![], home.clone(), |_| {}).await {
        Ok(_) => panic!("empty candidate list must fail"),
        Err(e) => assert!(e.contains("regent-deacon binary not found")),
    }
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn override_is_the_first_candidate() {
    // Only this test reads/writes REGENT_DEACON_PATH; the async tests use the
    // injected spawn core and never call deacon_candidates(), so there is no
    // concurrent env reader to race. Save/restore keeps the process pristine.
    let saved = std::env::var_os("REGENT_DEACON_PATH");
    let dir = std::env::temp_dir().join(format!("regent-cand-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mk temp");
    let bin = dir.join(super::locate::deacon_name());
    std::fs::write(&bin, b"x").expect("write");

    std::env::set_var("REGENT_DEACON_PATH", &bin);
    let cands = super::locate::deacon_candidates();
    assert_eq!(
        cands.first().map(PathBuf::as_path),
        Some(bin.as_path()),
        "the REGENT_DEACON_PATH override must be probed first"
    );

    match saved {
        Some(v) => std::env::set_var("REGENT_DEACON_PATH", v),
        None => std::env::remove_var("REGENT_DEACON_PATH"),
    }
    std::fs::remove_dir_all(&dir).ok();
}
