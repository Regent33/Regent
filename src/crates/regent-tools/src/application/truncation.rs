//! Gap T6: oversized tool results never enter history raw — the model gets
//! the head plus a spill receipt it can `read_file` on demand. Split from
//! `catalog.rs` for the file-size cap; called only from `dispatch`.

use crate::domain::entities::ToolContext;
use std::sync::atomic::{AtomicU64, Ordering};

/// Results past this length (chars, ≈7.5k tokens) are truncated; the full
/// bytes spill to the context's scratch dir when one is set.
const RESULT_CAP_CHARS: usize = 30_000;

/// Caps `result` at [`RESULT_CAP_CHARS`], spilling the full bytes to
/// `<scratch_dir>/<seq>-<tool>.txt` when the context has a scratch area.
/// A failed spill degrades to head-only truncation — never to an error.
pub(crate) fn truncate_oversized(
    seq: &AtomicU64,
    name: &str,
    result: String,
    ctx: &ToolContext,
) -> String {
    if result.chars().count() <= RESULT_CAP_CHARS {
        return result;
    }
    let receipt = ctx.scratch_dir.as_ref().and_then(|dir| {
        let n = seq.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("{n}-{name}.txt"));
        match std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, &result)) {
            Ok(()) => Some(path),
            Err(error) => {
                tracing::warn!(tool = name, %error, "spill of oversized tool result failed");
                None
            }
        }
    });
    let head: String = result.chars().take(RESULT_CAP_CHARS).collect();
    let marker = match receipt {
        Some(path) => format!(
            "\n[truncated — full output at {}; read_file it only if you need the rest]",
            path.display()
        ),
        None => "\n[truncated — output exceeded the result cap]".to_owned(),
    };
    tracing::info!(
        tool = name,
        chars = result.chars().count(),
        "oversized tool result truncated"
    );
    format!("{head}{marker}")
}

#[cfg(test)]
mod tests {
    use crate::application::catalog::ToolCatalog;
    use crate::domain::contracts::{DenyAll, ToolExecutor};
    use crate::domain::entities::ToolContext;
    use async_trait::async_trait;
    use regent_kernel::{RegentError, ToolDefinition};
    use serde_json::{Value, json};
    use std::sync::Arc;

    struct Oversized;

    #[async_trait]
    impl ToolExecutor for Oversized {
        async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
            Ok(json!({"blob": "x".repeat(100_000)}).to_string())
        }
    }

    /// Gap T6: an oversized result arrives truncated with a spill receipt; the
    /// spill file holds the full bytes. Without a scratch dir, head-only.
    #[tokio::test]
    async fn oversized_results_truncate_and_spill() {
        let mut catalog = ToolCatalog::new();
        catalog
            .register(
                ToolDefinition {
                    name: "big".into(),
                    description: "test".into(),
                    parameters: json!({"type": "object"}),
                    toolset: "test".into(),
                },
                Arc::new(Oversized),
            )
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll))
            .with_scratch_dir(dir.path().to_path_buf());
        let out = catalog.dispatch("big", "{}", &ctx).await;
        assert!(
            out.chars().count() < 31_000,
            "truncated: {} chars",
            out.len()
        );
        assert!(out.contains("[truncated — full output at"));
        let spill = std::fs::read_dir(dir.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert!(spill.file_name().to_string_lossy().ends_with("-big.txt"));
        let full = std::fs::read_to_string(spill.path()).unwrap();
        assert_eq!(full.chars().count(), 100_000 + r#"{"blob":""}"#.len());

        // No scratch dir → truncate without a receipt path, never an error.
        let bare = ToolContext::new(std::env::temp_dir(), Arc::new(DenyAll));
        let out = catalog.dispatch("big", "{}", &bare).await;
        assert!(out.contains("[truncated — output exceeded the result cap]"));
    }
}
