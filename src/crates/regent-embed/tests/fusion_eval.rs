//! End-to-end retrieval eval with a pass/fail threshold (rag-architect
//! deliverable): proves the tri-modal pipeline recalls memory from
//! **paraphrase** queries that share no key words with the stored content —
//! the recall FTS5 structurally cannot achieve. Ignored by default (runs the
//! real ONNX model: network + ~90 MB). Run on demand:
//!   cargo test -p regent-embed -- --ignored

use regent_embed::FastEmbedProvider;
use regent_graph::evals::{EvalClass, GoldenCase, run_golden};
use regent_graph::{GraphMemory, Provenance};
use regent_store::Store;
use std::sync::Arc;

struct EvalPair {
    /// Stored memory.
    memory: &'static str,
    /// A semantic paraphrase with minimal lexical overlap.
    query: &'static str,
}

const EVAL_SET: &[EvalPair] = &[
    EvalPair {
        memory: "the user prefers dark mode in the editor",
        query: "what colour theme does the developer like",
    },
    EvalPair {
        memory: "the project deploys to aws lambda on every merge",
        query: "where does our code get shipped",
    },
    EvalPair {
        memory: "standup meetings happen at nine each monday morning",
        query: "when is the daily team sync",
    },
    EvalPair {
        memory: "the datastore is postgres with the pgvector extension",
        query: "which database are we running",
    },
    EvalPair {
        memory: "ralph is allergic to shellfish and peanuts",
        query: "what foods must we avoid for the user",
    },
];

/// recall@3 across the eval set, with or without the semantic lane — scored
/// through the shared `regent_graph::evals` harness.
fn recall_at_3(with_embedder: bool) -> f64 {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let mut memory = GraphMemory::new(Arc::clone(&store));
    if with_embedder {
        memory = memory.with_embedder(Arc::new(FastEmbedProvider::new().expect("model loads")));
    }

    let cases: Vec<GoldenCase> = EVAL_SET
        .iter()
        .enumerate()
        .map(|(i, pair)| {
            let name = format!("m{i}");
            memory
                .add_node("fact", &name, pair.memory, Provenance::UserStated, None, None)
                .unwrap();
            GoldenCase {
                query: pair.query.to_owned(),
                expected: vec![name],
                class: EvalClass::Paraphrase,
            }
        })
        .collect();

    let report = run_golden(&cases, 3, |query| {
        memory.retrieve(query, 3).unwrap().iter().map(|r| r.node.name.clone()).collect()
    })
    .expect("eval dataset is well-formed");
    report.recall
}

#[test]
#[ignore = "runs the real ONNX model (network + ~90MB) — cargo test -p regent-embed -- --ignored"]
fn fusion_lifts_paraphrase_recall_over_fts_only() {
    let fts_only = recall_at_3(false);
    let tri_modal = recall_at_3(true);
    println!("paraphrase recall@3 — FTS+graph: {fts_only:.2}  ·  tri-modal: {tri_modal:.2}");

    assert!(
        tri_modal > fts_only,
        "the vector lane must lift paraphrase recall (tri-modal {tri_modal:.2} vs FTS {fts_only:.2})"
    );
    assert!(
        tri_modal >= 0.80,
        "tri-modal paraphrase recall@3 should reach 0.80 (got {tri_modal:.2})"
    );
}
