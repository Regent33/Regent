//! Regent's half of the v5 paired memory measurement.
//!
//! Loads the frozen corpus through the SAME path a user's memory takes
//! (`GraphMemory::add_entry`, budget and all), embeds it with the real local
//! model, and runs the frozen queries through the real tri-modal retrieval.
//!
//! Protocol v5 §5: emits PARTS ONLY — ordered ids, the rendered text of each,
//! and Regent's join template. No tokenization, no truncation, no metrics. The
//! scorer does all of that for both systems in one place.
//!
//! Also emits the individual lane rankings for §7 baseline 9, straight from
//! `regent-store`'s public API rather than a reimplementation of the fusion.
//!
//!   recallbench <artifacts-dir> <seed> <arm> <corpus-file> <out.json>

use regent_graph::{GraphMemory, MemoryTarget};
use regent_kernel::contracts::embedding::EmbeddingProvider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const RAISED_CHARS: usize = 200_000;
const MODEL_ID: &str = "all-MiniLM-L6-v2";

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
    /// The product path: `retrieve`, uncapped. The scorer cuts this to each budget.
    ranked: Vec<String>,
    /// §7 baseline 9 — each seed lane on its own, for ablation.
    lane_fts: Vec<String>,
    lane_vec: Vec<String>,
}

#[derive(Serialize)]
struct Run {
    system: &'static str,
    arm: String,
    seed: u32,
    corpus: String,
    stored: Vec<String>,
    refused: Vec<String>,
    template: serde_json::Value,
    queries: Vec<QueryResult>,
    rendered: std::collections::BTreeMap<String, String>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("artifacts dir");
    let seed: u32 = args.next().expect("seed").parse().expect("seed");
    let arm = args.next().expect("arm");
    let corpus_file = args.next().expect("corpus file");
    let out = args.next().expect("out path");

    let read = |name: &str| std::fs::read_to_string(format!("{dir}/{name}")).unwrap();
    let corpus: Vec<Entry> = serde_json::from_str(&read(&corpus_file)).unwrap();
    let queries: Vec<Query> = serde_json::from_str(&read("queries.json")).unwrap();
    // Shared seed file, so both systems see a byte-identical sequence.
    let order: Vec<String> = serde_json::from_str(&read(&format!("order-seed{seed}.json"))).unwrap();
    let text_of: std::collections::HashMap<&str, &str> = corpus
        .iter()
        .map(|e| (e.id.as_str(), e.text.as_str()))
        .collect();

    let store = Arc::new(regent_store::Store::open_in_memory().unwrap());
    let embedder: Arc<dyn EmbeddingProvider> =
        Arc::new(regent_embed::FastEmbedProvider::new().expect("embedder"));
    // §3.1 shipped = documented defaults, no `with_budgets`. §3.2 raised = the
    // same public API at 200k. Neither is a patch; both are configuration.
    let mut graph = GraphMemory::new(Arc::clone(&store));
    if arm == "raised" {
        graph = graph.with_budgets(RAISED_CHARS, RAISED_CHARS);
    }
    let graph = graph.with_embedder(Arc::clone(&embedder));

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

    // §3.2 — capacity must be neutralised in the raised arm, or delivery is not
    // what is being compared. Fatal, deliberately.
    assert!(
        arm != "raised" || refused.is_empty(),
        "raised arm refused {} of {} entries; capacity not neutralised (protocol §3.2)",
        refused.len(),
        order.len()
    );

    let backfilled = graph.backfill_embeddings(10_000).unwrap_or(0);
    let vectors = store.embedding_count(MODEL_ID).unwrap_or(0);
    eprintln!(
        "regent {arm} s{seed} {corpus_file}: stored {} refused {} | vectors {vectors} (+{backfilled})",
        stored.len(),
        refused.len()
    );
    // §8 — suspect the harness, on BOTH sides. An incomplete vector lane would
    // silently make this an FTS-only run and understate Regent.
    assert!(
        vectors == stored.len(),
        "vector lane incomplete ({vectors} of {})",
        stored.len()
    );

    let id_of = |content: &str, node: &str| -> String {
        by_text
            .get(content)
            .cloned()
            .unwrap_or_else(|| format!("?{node}"))
    };
    // Ask for everything so the TOKEN BUDGET is the only thing that truncates.
    // Taking a fixed k here is the policy confound the v2 review caught.
    let k = corpus.len();

    let results: Vec<QueryResult> = queries
        .iter()
        .map(|q| {
            let ranked: Vec<String> = graph
                .retrieve(&q.text, k)
                .unwrap_or_default()
                .iter()
                .map(|h| id_of(&h.node.content, &h.node.id))
                .collect();

            // Lane 1 alone — FTS5/BM25, through Regent's own recall grammar.
            let fts_q = regent_store::natural_fts_query(&q.text);
            let lane_fts: Vec<String> = store
                .fts_nodes(&fts_q, k as u32)
                .unwrap_or_default()
                .iter()
                .filter_map(|id| store.find_node(id).ok().flatten())
                .map(|n| id_of(&n.content, &n.id))
                .collect();

            // Lane 2 alone — vector cosine over the same store.
            let lane_vec: Vec<String> = match embedder.embed(&[q.text.clone()]) {
                Ok(v) if !v.is_empty() => store
                    .vector_search(&v[0], MODEL_ID, k)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|(id, _)| store.find_node(id).ok().flatten())
                    .map(|n| id_of(&n.content, &n.id))
                    .collect(),
                _ => Vec::new(),
            };

            for list in [&ranked, &lane_fts, &lane_vec] {
                assert!(
                    !list.iter().any(|i| i.starts_with('?')),
                    "unknown id in a Regent ranking — hard error per protocol §8"
                );
                let uniq: std::collections::HashSet<&String> = list.iter().collect();
                assert_eq!(uniq.len(), list.len(), "id ranked twice");
            }

            QueryResult {
                id: q.id.clone(),
                gold: q.gold.clone(),
                ranked,
                lane_fts,
                lane_vec,
            }
        })
        .collect();

    let rendered: std::collections::BTreeMap<String, String> = stored
        .iter()
        .map(|id| (id.clone(), text_of[id.as_str()].to_owned()))
        .collect();

    let run = Run {
        system: "regent",
        arm,
        seed,
        corpus: corpus_file,
        stored,
        refused,
        // Regent's recall renderer joins entries one per line under a header.
        template: serde_json::json!({
            "prefix": "", "separator": "\n", "suffix": ""
        }),
        queries: results,
        rendered,
    };
    std::fs::write(&out, serde_json::to_string_pretty(&run).unwrap()).unwrap();
    println!("wrote {out}");
}
