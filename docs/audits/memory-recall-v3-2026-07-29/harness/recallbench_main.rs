//! Regent's half of the v3 paired memory measurement.
//!
//! Loads the frozen corpus through the SAME path a user's memory takes
//! (`GraphMemory::add_entry`, budget and all), embeds it with the real local
//! model, and runs the frozen queries through the real tri-modal retrieval.
//!
//! Emits the RAW ranked list and computes no metrics: truncation to a token
//! budget is the scorer's job, so the delivery rule lives in exactly one place
//! for both systems.
//!
//! The cap is set through `with_budgets`, Regent's own public API — the mirror
//! of Hermes's `MemoryStore(memory_char_limit=...)`. Neither system is patched;
//! both are configured.
//!
//!   recallbench <artifacts-dir> <seed> <cap> <arm> <out.json>

use regent_graph::{GraphMemory, MemoryTarget};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
struct Entry {
    id: String,
    text: String,
}

#[derive(Deserialize)]
struct Query {
    id: String,
    text: String,
    gold: Vec<String>,
}

#[derive(Serialize)]
struct QueryResult {
    id: String,
    gold: Vec<String>,
    /// Corpus ids, best-first. The scorer cuts this to each token budget.
    ranked: Vec<String>,
}

#[derive(Serialize)]
struct Run {
    system: &'static str,
    arm: String,
    seed: u32,
    cap: usize,
    stored: Vec<String>,
    refused: Vec<String>,
    queries: Vec<QueryResult>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("artifacts dir");
    let seed: u32 = args.next().expect("seed").parse().expect("seed");
    let cap: usize = args.next().expect("cap").parse().expect("cap");
    let arm = args.next().expect("arm");
    let out = args.next().expect("out path");

    let corpus: Vec<Entry> =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/corpus.json")).unwrap())
            .unwrap();
    let queries: Vec<Query> =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/queries.json")).unwrap())
            .unwrap();
    // Shared seed file, so both systems see a byte-identical sequence.
    let order: Vec<String> = serde_json::from_str(
        &std::fs::read_to_string(format!("{dir}/order-seed{seed}.json")).unwrap(),
    )
    .unwrap();
    let text_of: std::collections::HashMap<&str, &str> = corpus
        .iter()
        .map(|e| (e.id.as_str(), e.text.as_str()))
        .collect();

    let store = Arc::new(regent_store::Store::open_in_memory().unwrap());
    let embedder = Arc::new(regent_embed::FastEmbedProvider::new().expect("embedder"));
    let graph = GraphMemory::new(Arc::clone(&store))
        .with_budgets(cap, cap)
        .with_embedder(embedder);

    let mut by_text = std::collections::HashMap::new();
    let (mut stored, mut refused) = (Vec::new(), Vec::new());
    for id in &order {
        let text = text_of[id.as_str()];
        by_text.insert(text.to_owned(), id.clone());
        match graph.add_entry(MemoryTarget::Memory, text) {
            Ok(_) => stored.push(id.clone()),
            Err(_) => refused.push(id.clone()),
        }
    }

    let backfilled = graph.backfill_embeddings(10_000).unwrap_or(0);
    let vectors = store.embedding_count("all-MiniLM-L6-v2").unwrap_or(0);
    eprintln!(
        "regent {arm} s{seed} cap={cap}: stored {} refused {} | vectors {vectors} (backfill +{backfilled})",
        stored.len(),
        refused.len()
    );
    // Protocol §7: suspect the harness, on BOTH sides. An incomplete vector lane
    // would silently make this an FTS-only run and understate Regent.
    assert!(
        vectors == stored.len(),
        "vector lane incomplete ({vectors} of {})",
        stored.len()
    );

    // Ask for the whole corpus so the TOKEN BUDGET is the only thing that
    // truncates. Taking a fixed k here is the policy confound the v2 review
    // caught — it made "waste" a number I chose rather than measured.
    let k = corpus.len();
    let results = queries
        .iter()
        .map(|q| QueryResult {
            id: q.id.clone(),
            gold: q.gold.clone(),
            ranked: graph
                .retrieve(&q.text, k)
                .unwrap_or_default()
                .iter()
                .map(|h| {
                    by_text
                        .get(&h.node.content)
                        .cloned()
                        .unwrap_or_else(|| format!("?{}", h.node.id))
                })
                .collect(),
        })
        .collect();

    let run = Run {
        system: "regent",
        arm,
        seed,
        cap,
        stored,
        refused,
        queries: results,
    };
    std::fs::write(&out, serde_json::to_string_pretty(&run).unwrap()).unwrap();
    println!("wrote {out}");
}
