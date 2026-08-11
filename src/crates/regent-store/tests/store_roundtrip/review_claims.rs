use regent_kernel::SessionId;
use regent_store::{ReviewClaimOutcome, Store};
use std::sync::{Arc, Barrier};

fn session(store: &Store) -> SessionId {
    let id = SessionId::generate();
    store
        .create_session(&id, "deacon", None, Some("prompt"), None)
        .unwrap();
    id
}

#[test]
fn simultaneous_processes_have_one_review_claim_winner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let first_store = Store::open(&path).unwrap();
    let id = session(&first_store);
    let second_store = Store::open(&path).unwrap();
    let barrier = Arc::new(Barrier::new(3));

    let first = {
        let id = id.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            first_store.claim_session_review_range(&id, 8, 120.0)
        })
    };
    let second = {
        let id = id.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            second_store.claim_session_review_range(&id, 8, 120.0)
        })
    };
    barrier.wait();
    let outcomes = [
        first.join().unwrap().unwrap(),
        second.join().unwrap().unwrap(),
    ];

    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ReviewClaimOutcome::Acquired(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ReviewClaimOutcome::Busy))
            .count(),
        1
    );
}

#[test]
fn expired_claim_retries_and_stale_owner_cannot_finish_new_lease() {
    let store = Store::open_in_memory().unwrap();
    let id = session(&store);
    let first = match store.claim_session_review_range(&id, 2, 0.0).unwrap() {
        ReviewClaimOutcome::Acquired(lease) => lease,
        other => panic!("first claim should acquire, got {other:?}"),
    };
    let second = match store.claim_session_review_range(&id, 2, 120.0).unwrap() {
        ReviewClaimOutcome::Acquired(lease) => lease,
        other => panic!("expired claim should be reclaimed, got {other:?}"),
    };

    assert_ne!(first.token, second.token);
    assert!(
        !store
            .finish_session_review_claim(&id, &first.token)
            .unwrap()
    );
    assert!(
        store
            .renew_session_review_claim(&id, &second.token, 120.0)
            .unwrap()
    );
    assert!(
        store
            .release_session_review_claim(&id, &second.token)
            .unwrap()
    );

    let final_lease = match store.claim_session_review_range(&id, 2, 120.0).unwrap() {
        ReviewClaimOutcome::Acquired(lease) => lease,
        other => panic!("released failure should retry, got {other:?}"),
    };
    assert!(
        store
            .finish_session_review_claim(&id, &final_lease.token)
            .unwrap()
    );
    assert_eq!(store.session_reviewed_message_count(&id).unwrap(), 2);
    assert!(matches!(
        store.claim_session_review_range(&id, 2, 120.0).unwrap(),
        ReviewClaimOutcome::Covered {
            reviewed_message_count: 2
        }
    ));
}
