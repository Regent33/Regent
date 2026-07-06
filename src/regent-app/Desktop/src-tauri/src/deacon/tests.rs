//! End-to-end smoke test against the REAL deacon binary: spawn it, round-trip
//! `status.get`, then shut it down cleanly. Skipped (not failed) when the
//! release binary is absent so a fresh checkout doesn't red the suite.

use super::spawn;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

/// Repo `target/release/regent-deacon.exe`, relative to this crate (src-tauri
/// is four levels below the repo root: Desktop → regent-app → src → repo).
fn release_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("..")
        .join("target")
        .join("release")
        .join("regent-deacon.exe")
}

#[tokio::test]
async fn status_get_round_trips_against_real_deacon() {
    let bin = release_binary();
    if !bin.exists() {
        eprintln!("skipping: release deacon not found at {}", bin.display());
        return;
    }

    // Isolate REGENT_HOME so the test never touches the user's real ~/.regent,
    // and pin the binary via the env override. (edition 2021 → set_var is safe.)
    let home = std::env::temp_dir().join(format!(
        "regent-desktop-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&home).expect("create temp REGENT_HOME");
    std::env::set_var("REGENT_HOME", &home);
    std::env::set_var("REGENT_DEACON_PATH", &bin);

    let (rpc, mut child) = spawn::spawn(|_line| {})
        .await
        .expect("spawn real deacon");

    let resp = tokio::time::timeout(
        Duration::from_secs(15),
        rpc.request("status.get", json!({})),
    )
    .await
    .expect("status.get did not answer within 15s")
    .expect("status.get returned an error");

    // A JSON-RPC response with the matching id (the first request is id 1) and
    // either a result or an error payload.
    assert_eq!(resp["id"], json!(1));
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "expected a JSON-RPC result/error, got {resp}"
    );

    // Graceful drain: stdin EOF → 2s grace → kill.
    rpc.close_stdin().await;
    if tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .is_err()
    {
        child.kill().await.ok();
    }
    std::fs::remove_dir_all(&home).ok();
}
