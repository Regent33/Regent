//! Partitioned tool dispatch (gap L3): contiguous runs of read-only calls
//! execute in parallel; mutating calls execute serially, in call order — two
//! file_edits on the same file (or an edit racing the build in `terminal`)
//! must never interleave. Results re-attach in original call order either way
//! (runs execute in order; join_all preserves input order within a run).
//! Split from `turn.rs` (file-size rule).

use futures::future::join_all;
use regent_kernel::ToolCall;
use regent_tools::{ToolCatalog, ToolContext};
use std::sync::Arc;

pub(super) async fn dispatch_partitioned(
    catalog: &Arc<ToolCatalog>,
    ctx: &ToolContext,
    calls: &[ToolCall],
) -> Vec<String> {
    let mut results: Vec<String> = Vec::with_capacity(calls.len());
    let mut start = 0;
    while start < calls.len() {
        let read_only = regent_kernel::is_read_only_tool(&calls[start].name);
        let mut end = start + 1;
        while end < calls.len() && regent_kernel::is_read_only_tool(&calls[end].name) == read_only {
            end += 1;
        }
        if read_only {
            let dispatches = calls[start..end]
                .iter()
                .map(|call| catalog.dispatch(&call.name, &call.arguments, ctx));
            results.extend(join_all(dispatches).await);
        } else {
            for call in &calls[start..end] {
                results.push(catalog.dispatch(&call.name, &call.arguments, ctx).await);
            }
        }
        start = end;
    }
    results
}
