//! Unit tests for `graph` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

fn node(id: &str, updated_at: f64) -> NodeRow {
    NodeRow {
        id: id.into(),
        kind: "fact".into(),
        name: id.into(),
        content: "content".into(),
        provenance: "user_stated".into(),
        trust: 0.9,
        session_id: None,
        created_at: updated_at,
        updated_at,
        ttl_expires_at: None,
        access_count: 0,
        content_hash: format!("hash-{id}"),
    }
}

#[test]
fn list_nodes_newest_first_and_edges_stay_within_the_set() {
    let store = Store::open_in_memory().unwrap();
    store.insert_node(&node("a", 10.0)).unwrap();
    store.insert_node(&node("b", 30.0)).unwrap();
    store.insert_node(&node("c", 20.0)).unwrap();
    store
        .upsert_edge("a", "b", "relates_to", 1.0, "agent_inferred")
        .unwrap();

    // Most-recently-updated first.
    let nodes = store.list_nodes(10).unwrap();
    let ids: Vec<_> = nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["b", "c", "a"]);

    // Both endpoints are within the returned node set → edge is included.
    let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
    let edges = store.list_edges_among(&node_ids).unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!((edges[0].src.as_str(), edges[0].dst.as_str()), ("a", "b"));

    // An endpoint outside the selected set excludes the edge.
    assert!(
        store
            .list_edges_among(&["b".to_string()])
            .unwrap()
            .is_empty()
    );
    assert!(store.list_edges_among(&[]).unwrap().is_empty());
}
