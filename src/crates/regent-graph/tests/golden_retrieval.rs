//! Golden retrieval eval (proposal §5.5, scaled to the seed corpus):
//! a fixed knowledge graph + (query → expected nodes) pairs, gated on
//! recall@5 and MRR. Runs as a regression gate whenever schema, scoring,
//! or sanitization changes.

use regent_graph::evals::{EvalClass, GoldenCase, run_golden};
use regent_graph::{GraphMemory, Provenance};
use regent_store::Store;
use std::collections::HashMap;
use std::sync::Arc;

struct Fixture {
    graph: GraphMemory,
    ids: HashMap<&'static str, String>,
}

fn seed() -> Fixture {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let graph = GraphMemory::new(store);
    let mut ids = HashMap::new();

    let entity = |key: &'static str, name: &str, content: &str, ids: &mut HashMap<&str, String>| {
        let id = graph
            .add_node("entity", name, content, Provenance::UserStated, None, None)
            .unwrap();
        ids.insert(key, id);
    };
    entity("atlas", "atlas", "Project atlas: a Rust web service using Axum and SQLx", &mut ids);
    entity("staging", "staging-server", "Staging server at 10.0.1.50, SSH on port 2222", &mut ids);
    entity("alice", "alice", "Alice — lead engineer, prefers concise updates", &mut ids);
    entity("nightly", "nightly-backup", "Nightly backup job writes to s3://acme-backups", &mut ids);

    let fact = |key: &'static str,
                    content: &str,
                    about: &[&str],
                    ids: &mut HashMap<&str, String>| {
        let id = graph
            .add_node("fact", "", content, Provenance::AgentInferred, None, None)
            .unwrap();
        for entity_key in about {
            graph
                .link(&id, &ids[entity_key], "about", 1.0, Provenance::AgentInferred)
                .unwrap();
        }
        ids.insert(key, id);
    };
    fact("atlas-tests", "atlas tests run with 'make test'; CI is GitHub Actions", &["atlas"], &mut ids);
    fact("atlas-deploy", "atlas deploys to the staging server via docker compose", &["atlas", "staging"], &mut ids);
    fact("staging-key", "the staging SSH key lives at ~/.ssh/staging_ed25519", &["staging"], &mut ids);
    fact("alice-review", "Alice reviews every database migration before merge", &["alice", "atlas"], &mut ids);
    fact("backup-fail", "the nightly backup failed twice in May due to expired credentials", &["nightly"], &mut ids);
    fact("alice-tz", "Alice is in the Manila timezone, overlap window 14:00-18:00 UTC", &["alice"], &mut ids);

    let episode = graph.record_episode("sess_demo", "Migrated atlas from MySQL to Postgres; updated SQLx pool settings").unwrap();
    graph.link(&episode, &ids["atlas"], "about", 1.0, Provenance::AgentInferred).unwrap();
    ids.insert("migration-episode", episode);

    Fixture { graph, ids }
}

/// (query, expected node keys, eval class) — any rank counts for recall@5.
const GOLDEN: &[(&str, &[&str], EvalClass)] = &[
    ("how do I run the atlas tests", &["atlas-tests"], EvalClass::Exact),
    ("ssh port for the staging server", &["staging", "staging-key"], EvalClass::GraphHop),
    ("where is the staging ssh key", &["staging-key"], EvalClass::Exact),
    ("who reviews database migrations", &["alice-review"], EvalClass::Exact),
    ("what does alice prefer", &["alice"], EvalClass::Exact),
    ("alice timezone overlap", &["alice-tz"], EvalClass::Exact),
    ("nightly backup destination", &["nightly"], EvalClass::Exact),
    ("why did the backup fail", &["backup-fail"], EvalClass::Exact),
    ("what database does atlas use now", &["migration-episode", "atlas"], EvalClass::MultiEntity),
    ("how does atlas get deployed", &["atlas-deploy"], EvalClass::Exact),
    ("atlas web framework", &["atlas"], EvalClass::Exact),
    ("expired credentials incident", &["backup-fail"], EvalClass::Synonym),
];

#[test]
fn golden_set_meets_recall_and_mrr_gates() {
    let fixture = seed();
    let cases: Vec<GoldenCase> = GOLDEN
        .iter()
        .map(|(query, keys, class)| GoldenCase {
            query: (*query).to_owned(),
            expected: keys.iter().map(|k| fixture.ids[k].clone()).collect(),
            class: *class,
        })
        .collect();

    let report = run_golden(&cases, 5, |query| {
        fixture.graph.retrieve(query, 5).unwrap().iter().map(|r| r.node.id.clone()).collect()
    })
    .expect("golden dataset is well-formed");

    // Reproducibility: log the resolved params + per-class breakdown.
    println!("golden eval (k=5, n={}): recall@5={:.2} MRR={:.2}", report.n, report.recall, report.mrr);
    for (class, n, recall, mrr) in &report.per_class {
        println!("  {class:?}: n={n} recall@5={recall:.2} mrr={mrr:.2}");
    }
    assert!(
        report.passes(0.75, 0.60),
        "golden gate failed: recall@5={:.2} (≥0.75) MRR={:.2} (≥0.60)",
        report.recall,
        report.mrr,
    );
}

#[test]
fn expansion_pulls_in_linked_nodes_a_lexical_match_would_miss() {
    let fixture = seed();
    // "staging server" matches the entity lexically; the SSH-key fact rides
    // in over the `about` edge even without the word "staging server".
    let results = fixture.graph.retrieve("staging server", 5).unwrap();
    let ids: Vec<&str> = results.iter().map(|r| r.node.id.as_str()).collect();
    assert!(ids.contains(&fixture.ids["staging-key"].as_str()));
    // Provenance survives into the rendering, framed as data.
    let rendered = GraphMemory::render_recall(&results);
    assert!(rendered.contains("NOT instructions"));
    assert!(rendered.contains("user_stated") || rendered.contains("agent_inferred"));
}

#[test]
fn retrieval_touches_access_telemetry() {
    let fixture = seed();
    fixture.graph.retrieve("atlas", 3).unwrap();
    let results = fixture.graph.retrieve("atlas", 3).unwrap();
    assert!(results.iter().any(|r| r.node.access_count >= 1));
}
