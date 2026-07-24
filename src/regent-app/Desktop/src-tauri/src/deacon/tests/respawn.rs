//! Respawn-on-death and single-flight: a dead deacon is replaced on the next
//! request (what the front-end "Retry" exercises), and concurrent cold requests
//! share ONE spawn rather than racing two children.

use super::super::{spawn, DeaconHandle, DeaconRpc, DeaconState};
use super::{repo_deacon, scratch_home};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Poll until the client observes its deacon as dead (bounded).
async fn wait_dead(rpc: &Arc<DeaconRpc>) -> bool {
    for _ in 0..200 {
        if rpc.is_dead() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    rpc.is_dead()
}

#[tokio::test]
async fn respawns_a_dead_deacon() {
    let Some(bin) = repo_deacon() else {
        eprintln!("skipping: no built regent-deacon");
        return;
    };
    let home = scratch_home("respawn");

    // Spawn a real deacon, then hard-kill it so it is promptly dead (a crash),
    // and seed the managed state with that dead handle.
    let (rpc1, mut child1) = spawn::spawn_with(vec![bin.clone()], home.clone(), |_| {})
        .await
        .expect("cold spawn");
    child1.kill().await.ok();
    assert!(wait_dead(&rpc1).await, "killed deacon should be observed dead");
    let state = DeaconState {
        inner: Mutex::new(Some(DeaconHandle {
            rpc: Arc::clone(&rpc1),
            child: child1,
        })),
    };

    // A request now respawns a FRESH live client (what the Retry button does),
    // not a re-hit of the dead pipe.
    let rpc2 = {
        let (bin, home) = (bin.clone(), home.clone());
        state
            .ensure_client_with(|| spawn::spawn_with(vec![bin], home, |_| {}))
            .await
            .expect("respawn")
    };
    assert!(!rpc2.is_dead());
    assert!(!Arc::ptr_eq(&rpc1, &rpc2), "respawn must be a new client");
    let resp = rpc2.request("status.get", json!({})).await.expect("status");
    assert!(resp.get("result").is_some() || resp.get("error").is_some());

    // Drop `state` → kill_on_drop reaps the live child (fast; no graceful drain).
    drop(state);
    std::fs::remove_dir_all(&home).ok();
}

#[tokio::test]
async fn concurrent_first_requests_spawn_once() {
    let Some(bin) = repo_deacon() else {
        eprintln!("skipping: no built regent-deacon");
        return;
    };
    let home = scratch_home("single-flight");
    let state = DeaconState {
        inner: Mutex::new(None),
    };

    // Two requests race the cold start; the state mutex must single-flight the
    // spawn so both share ONE deacon (no duplicate concurrent child).
    let (a, b) = tokio::join!(
        state.ensure_client_with(|| spawn::spawn_with(vec![bin.clone()], home.clone(), |_| {})),
        state.ensure_client_with(|| spawn::spawn_with(vec![bin.clone()], home.clone(), |_| {})),
    );
    let a = a.expect("spawn a");
    let b = b.expect("spawn b");
    assert!(
        Arc::ptr_eq(&a, &b),
        "concurrent cold requests must share a single spawned deacon"
    );

    drop(state);
    std::fs::remove_dir_all(&home).ok();
}
