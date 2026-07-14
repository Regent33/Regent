//! The `memory` tool's action dispatch (remember/pending/approve/forget…).
//! Split from `memory_tools.rs` (file-size rule).

use super::*;

/// Seven days for an external write proposal to be approved before it expires.
const PENDING_WRITE_TTL_SECS: f64 = 7.0 * 86_400.0;

pub(super) fn run_memory_action(graph: &GraphMemory, args: &Value, external: bool) -> String {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = match MemoryTarget::parse(
        args.get("target")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    ) {
        Ok(target) => target,
        Err(error) => return tool_error_json(error.to_string()),
    };
    let content = args.get("content").and_then(Value::as_str);
    let old_text = args.get("old_text").and_then(Value::as_str);

    if external {
        // External sessions may only PROPOSE additions (staged for the owner
        // to approve via `memory.pending`/`memory.approve`); edits to what is
        // already trusted memory are refused outright.
        return match (action, content) {
            ("add", Some(content)) => match graph.stage_write(
                regent_graph::ENTRY_KIND,
                target.kind(),
                content,
                Provenance::AgentInferred,
                None,
                Some(PENDING_WRITE_TTL_SECS),
            ) {
                Ok(id) => json!({
                    "success": true,
                    "result": format!(
                        "queued for the owner's approval (id {id}); it is NOT saved yet"),
                })
                .to_string(),
                Err(error) => tool_error_json(error.to_string()),
            },
            _ => tool_error_json(
                "memory edits from an externally-triggered session require the owner: \
                 only 'add' is accepted here, and it is queued for approval",
            ),
        };
    }

    let outcome = match (action, content, old_text) {
        ("add", Some(content), _) => graph.add_entry(target, content).map(|added| match added {
            AddOutcome::Added => "saved".to_owned(),
            AddOutcome::Duplicate => "already stored — no duplicate added".to_owned(),
        }),
        ("replace", Some(content), Some(old_text)) => graph
            .replace_entry(target, old_text, content)
            .map(|()| "replaced".to_owned()),
        ("remove", _, Some(old_text)) => graph
            .remove_entry(target, old_text)
            .map(|()| "removed".to_owned()),
        _ => {
            return tool_error_json(
                "invalid arguments: add needs content; replace needs old_text + content; \
             remove needs old_text",
            );
        }
    };

    match outcome {
        Ok(message) => {
            let (used, limit) = graph.usage(target).unwrap_or((0, 0));
            json!({"success": true, "result": message, "usage": format!("{used}/{limit}")})
                .to_string()
        }
        // The budget error carries current entries so the agent can
        // consolidate in the same turn (never auto-compacted).
        Err(GraphError::BudgetExceeded {
            used,
            limit,
            attempted,
            entries,
        }) => json!({
            "success": false,
            "error": format!(
                "Memory at {used}/{limit} chars. This write ({attempted} chars) would exceed \
                 the limit. Consolidate now: 'replace' overlapping entries with shorter ones or \
                 'remove' stale ones (see current_entries), then retry — all in this turn."),
            "current_entries": entries,
            "usage": format!("{used}/{limit}"),
        })
        .to_string(),
        Err(error) => tool_error_json(error.to_string()),
    }
}
