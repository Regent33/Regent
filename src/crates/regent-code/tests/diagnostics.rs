//! Gap H5 acceptance: an edit that breaks the build gets the checker's
//! findings attached to its own tool result as a `diagnostics` field; clean
//! edits and no-manifest workspaces get none; a failing edit is never made
//! worse by diagnostics.

use async_trait::async_trait;
use regent_code::wrap_diagnostics;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use regent_tools::{DenyAll, ToolCatalog, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

/// Test double standing in for `write_file`: actually writes the file (so the
/// checker sees the breakage) and returns the usual JSON-object result.
struct WritesForReal;

#[async_trait]
impl ToolExecutor for WritesForReal {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let path = args["path"].as_str().unwrap_or_default();
        let content = args["content"].as_str().unwrap_or_default();
        if content == "TRIGGER-ERROR" {
            return Ok(tool_error_json("simulated edit failure"));
        }
        std::fs::write(ctx.cwd.join(path), content)
            .map_err(|e| RegentError::Store(e.to_string()))?;
        Ok(json!({"path": path, "written": true}).to_string())
    }
}

fn catalog_for(workspace: &Path) -> ToolCatalog {
    let mut catalog = ToolCatalog::new();
    catalog
        .register(
            ToolDefinition {
                name: "write_file".into(),
                description: "test double".into(),
                parameters: json!({"type": "object"}),
                toolset: "file".into(),
            },
            Arc::new(WritesForReal),
        )
        .unwrap();
    wrap_diagnostics(&mut catalog, workspace);
    catalog
}

fn min_crate(dir: &Path) {
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
}

fn ctx(dir: &Path) -> ToolContext {
    ToolContext::new(dir.to_owned(), Arc::new(DenyAll))
}

#[tokio::test]
async fn broken_edit_carries_diagnostics_clean_edit_does_not() {
    let dir = tempfile::tempdir().unwrap();
    min_crate(dir.path());
    let catalog = catalog_for(dir.path());

    // A type error lands in the same tool result that made it.
    let broken = json!({"path": "src/lib.rs", "content": "pub fn ok() -> u32 { \"nope\" }\n"});
    let result = catalog
        .dispatch("write_file", &broken.to_string(), &ctx(dir.path()))
        .await;
    let value: Value = serde_json::from_str(&result).expect("result stays well-formed JSON");
    let diag = &value["diagnostics"];
    assert_eq!(
        diag["file"], "src/lib.rs",
        "diagnostics name the file: {result}"
    );
    assert!(
        diag["output"].as_str().unwrap().contains("E0308")
            || diag["output"].as_str().unwrap().contains("mismatched"),
        "checker output attached: {result}"
    );

    // Fixing it back → no diagnostics field.
    let clean = json!({"path": "src/lib.rs", "content": "pub fn ok() {}\n"});
    let result = catalog
        .dispatch("write_file", &clean.to_string(), &ctx(dir.path()))
        .await;
    let value: Value = serde_json::from_str(&result).unwrap();
    assert!(
        value.get("diagnostics").is_none(),
        "clean edit stays clean: {result}"
    );
}

#[tokio::test]
async fn no_manifest_workspace_and_failed_edits_skip_the_check() {
    let dir = tempfile::tempdir().unwrap();
    // No Cargo.toml at all → .rs edits get no diagnostics (nothing to run).
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    let catalog = catalog_for(dir.path());
    let args = json!({"path": "src/lib.rs", "content": "not even rust"});
    let result = catalog
        .dispatch("write_file", &args.to_string(), &ctx(dir.path()))
        .await;
    let value: Value = serde_json::from_str(&result).unwrap();
    assert!(value.get("diagnostics").is_none(), "{result}");

    // An edit that itself failed passes through untouched — never re-judged.
    let failing = json!({"path": "src/lib.rs", "content": "TRIGGER-ERROR"});
    let result = catalog
        .dispatch("write_file", &failing.to_string(), &ctx(dir.path()))
        .await;
    let value: Value = serde_json::from_str(&result).unwrap();
    assert!(value.get("error").is_some());
    assert!(value.get("diagnostics").is_none());
}
