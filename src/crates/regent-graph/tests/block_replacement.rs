//! W3 step 4 — is entry-scoped retrieval a viable replacement for the
//! always-injected memory block?
//!
//! Step 3 measured the UNSCOPED selection against a real corpus and it failed
//! the gate: 2.59x the block's cost, 83% coverage, with `constitution` taking
//! half of every selection — content the block never carried, already always-on
//! by its own path. This measures the like-for-like version.
//!
//! **Retrieval needs no model.** It is deterministic given the corpus, so this
//! runs offline against a store rather than paying for a traffic run.
//!
//! Ignored by default: it needs a populated store, which CI does not have. Run
//! it against a COPY of a real one — never the live `$REGENT_HOME`:
//!
//! ```text
//! REGENT_EVAL_DB=/path/to/copy-of-state.db \
//!   cargo test -p regent-graph --test block_replacement -- --ignored --nocapture
//! ```

use regent_graph::{GraphMemory, MemoryTarget};
use regent_store::Store;
use std::collections::HashSet;
use std::sync::Arc;

/// The same queries the traffic run used: hits, deliberate misses, and partials.
/// The misses are load-bearing — without them this measures recall and never
/// precision, and a retriever that returns everything would look perfect.
const QUERIES: &[&str] = &[
    "What shell do I use on this machine?",
    "Remind me about the Philippine history decks I was making.",
    "What was the molecular biology website built with?",
    "What's my name?",
    "Where does Butler Mode live on disk?",
    "Have background tasks failed on me before?",
    "What is the capital of Portugal?",
    "Convert 40 degrees celsius to fahrenheit.",
    "Write a haiku about rain.",
    "How do I play music and schedule something for later?",
    "What study guides have I made?",
    "What is the airspeed velocity of an unladen swallow?",
];

const K: usize = 10;

#[test]
#[ignore = "needs REGENT_EVAL_DB pointing at a copy of a populated store"]
fn measure_block_replacement_cost() {
    let Ok(db) = std::env::var("REGENT_EVAL_DB") else {
        panic!("set REGENT_EVAL_DB to a COPY of a populated state.db");
    };
    let store = Arc::new(Store::open(std::path::Path::new(&db)).expect("open store"));
    let graph = GraphMemory::new(Arc::clone(&store));

    // The baseline: what the block costs on every turn, whatever was asked.
    let block: usize = graph
        .block_metrics()
        .expect("block metrics")
        .iter()
        .map(|m| m.chars)
        .sum();

    let block_ids: HashSet<String> = [MemoryTarget::Memory, MemoryTarget::User]
        .into_iter()
        .flat_map(|t| store.nodes_by_kind(t.kind()).expect("nodes"))
        .map(|n| n.id)
        .collect();

    let (mut unscoped_total, mut scoped_total) = (0usize, 0usize);
    let mut reached: HashSet<String> = HashSet::new();
    let mut scoped_slots = 0usize;

    println!("\n{:=<72}", "");
    println!("W3 step 4 — entry-scoped retrieval vs the static block");
    println!("{:=<72}\n", "");
    println!(
        "static block: {block} chars, every turn, {} entries",
        block_ids.len()
    );
    println!("\n{:<44} {:>10} {:>10}", "query", "unscoped", "scoped");

    for query in QUERIES {
        let shadow = graph.shadow_recall(query, K).expect("shadow");
        unscoped_total += shadow.would_inject_chars;
        scoped_total += shadow.entry_inject_chars;
        scoped_slots += shadow.entry_selected.len();
        for r in &shadow.entry_selected {
            reached.insert(r.node.id.clone());
        }
        let short: String = query.chars().take(42).collect();
        println!(
            "{:<44} {:>10} {:>10}",
            short, shadow.would_inject_chars, shadow.entry_inject_chars
        );
    }

    let n = QUERIES.len();
    let unscoped_mean = unscoped_total as f64 / n as f64;
    let scoped_mean = scoped_total as f64 / n as f64;
    let covered = reached.intersection(&block_ids).count();

    println!("\n{:-<72}", "");
    println!(
        "unscoped mean : {unscoped_mean:>8.0} chars  ({:.2}x the block)",
        unscoped_mean / block as f64
    );
    println!(
        "scoped   mean : {scoped_mean:>8.0} chars  ({:.2}x the block)",
        scoped_mean / block as f64
    );
    println!(
        "coverage      : {covered}/{} block entries reached ({:.0}%)",
        block_ids.len(),
        100.0 * covered as f64 / block_ids.len().max(1) as f64
    );
    println!(
        "mean entry slots filled: {:.1} of k={K}",
        scoped_slots as f64 / n as f64
    );
    println!("{:-<72}", "");

    // The gate, stated as an assertion so a future change cannot quietly
    // reintroduce the problem: scoped retrieval must at least be CHEAPER than
    // the block it proposes to replace. Cost is necessary, not sufficient —
    // coverage is reported above and is the owner's call.
    assert!(
        scoped_mean < block as f64,
        "entry-scoped retrieval costs {scoped_mean:.0} chars vs the block's {block} — \
         it is not a replacement, it is an addition"
    );
}
