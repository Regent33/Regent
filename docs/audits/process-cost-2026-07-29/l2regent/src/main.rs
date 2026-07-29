//! Regent's half of L2: identical memory work, through the same public API the
//! deacon uses. No patching — the raised cap goes in via `with_budgets`, the
//! mirror of Hermes's `MemoryStore(memory_char_limit=...)`.
//!
//!   l2regent <scratch-dir> <n> <raised-chars>

use regent_graph::{GraphMemory, MemoryTarget};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let mut a = std::env::args().skip(1);
    let dir = a.next().expect("scratch dir");
    let n: usize = a.next().expect("n").parse().unwrap();
    let raised: usize = a.next().expect("raised").parse().unwrap();
    std::fs::create_dir_all(&dir).unwrap();

    let t0 = Instant::now();
    let path = std::path::PathBuf::from(format!("{dir}/store.db"));
    let store = Arc::new(regent_store::Store::open(&path).unwrap());
    let graph = GraphMemory::new(store).with_budgets(raised, raised);
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t0 = Instant::now();
    let mut ok = 0usize;
    for i in 0..n {
        let text = format!("benchmark record {i:03} - fixed width padding to sixty chars.xx");
        if graph.add_entry(MemoryTarget::Memory, &text).is_ok() {
            ok += 1;
        }
    }
    let write_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t0 = Instant::now();
    let block = graph.render_prompt_block().unwrap_or_default();
    let render_ms = t0.elapsed().as_secs_f64() * 1000.0;

    println!(
        "{}",
        serde_json::json!({
            "open_ms": open_ms, "write_ms": write_ms, "render_ms": render_ms,
            "stored": ok, "block": block.chars().count(),
        })
    );
}
