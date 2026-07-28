//! Regent's half of the paired memory-recall pilot.
//!
//! Loads the frozen corpus through the SAME path a user's memory takes
//! (`GraphMemory::add_entry`, budget and all), embeds it with the real local
//! model, and runs the frozen queries through the real tri-modal retrieval.
//! Emits raw ranked output as JSON for the scorer — this binary computes no
//! metrics, so the scoring rule lives in exactly one place.
//!
//!   recallbench <artifacts-dir> <N> <out.json>

use regent_graph::{GraphMemory, MemoryTarget};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

#[derive(Deserialize)]
struct Entry {
    id: String,
    text: String,
}

#[derive(Deserialize)]
struct Query {
    id: String,
    kind: String,
    text: String,
    gold: Vec<String>,
}

#[derive(Serialize)]
struct QueryResult {
    id: String,
    kind: String,
    gold: Vec<String>,
    /// Corpus ids of the returned entries, best-first.
    returned: Vec<String>,
    ms: f64,
}

#[derive(Serialize)]
struct Run {
    system: &'static str,
    n: usize,
    /// Corpus ids the store actually accepted.
    stored: Vec<String>,
    refused: Vec<String>,
    queries: Vec<QueryResult>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("artifacts dir");
    let n: usize = args.next().expect("N").parse().expect("N");
    let out = args.next().expect("out path");

    let corpus: Vec<Entry> =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/corpus.json")).unwrap())
            .unwrap();
    let queries: Vec<Query> =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/queries.json")).unwrap())
            .unwrap();

    let store = Arc::new(regent_store::Store::open_in_memory().unwrap());
    let embedder = Arc::new(regent_embed::FastEmbedProvider::new().expect("embedder"));
    let graph = GraphMemory::new(Arc::clone(&store)).with_embedder(embedder);

    // Text -> corpus id, so results map back without polluting the indexed text.
    let mut by_text = std::collections::HashMap::new();
    let (mut stored, mut refused) = (Vec::new(), Vec::new());
    for entry in corpus.iter().take(n) {
        by_text.insert(entry.text.clone(), entry.id.clone());
        // The user's own path: same budget, same validation, same refusal.
        match graph.add_entry(MemoryTarget::Memory, &entry.text) {
            Ok(_) => stored.push(entry.id.clone()),
            Err(_) => refused.push(entry.id.clone()),
        }
    }
    // Embed whatever landed; retrieval fuses the vector lane only when present.
    let backfilled = graph.backfill_embeddings(10_000).unwrap_or(0);
    // The count that matters is how many vectors EXIST, not how many the
    // backfill added — writes embed inline when an embedder is attached, so a
    // backfill of 0 is the healthy case, not a silent empty vector lane.
    let vectors = store.embedding_count("all-MiniLM-L6-v2").unwrap_or(0);
    eprintln!(
        "N={n}: stored {} refused {} | vectors {vectors} (backfill added {backfilled})",
        stored.len(),
        refused.len()
    );
    assert!(
        vectors == stored.len(),
        "vector lane is incomplete ({vectors} of {}); retrieval would be FTS-only and the result would understate Regent",
        stored.len()
    );

    let results = queries
        .iter()
        .map(|q| {
            let start = Instant::now();
            let hits = graph.retrieve(&q.text, 10).unwrap_or_default();
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            QueryResult {
                id: q.id.clone(),
                kind: q.kind.clone(),
                gold: q.gold.clone(),
                returned: hits
                    .iter()
                    .map(|h| {
                        by_text
                            .get(&h.node.content)
                            .cloned()
                            .unwrap_or_else(|| format!("?{}", h.node.id))
                    })
                    .collect(),
                ms,
            }
        })
        .collect();

    let run = Run {
        system: "regent",
        n,
        stored,
        refused,
        queries: results,
    };
    std::fs::write(&out, serde_json::to_string_pretty(&run).unwrap()).unwrap();
    println!("wrote {out}");
}
