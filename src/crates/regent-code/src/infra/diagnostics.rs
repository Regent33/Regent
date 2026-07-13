//! Edit-time diagnostics (gap H5): after a successful `file_edit` /
//! `write_file` / `apply_patch`, run the cheap per-language check and attach
//! what broke to the SAME tool result, so the model reacts now instead of
//! stacking errors until the end-of-run verify. Two invariants:
//! diagnostics NEVER fail an edit (any spawn/timeout/parse problem degrades
//! to "no diagnostics", log-only), and results stay well-formed JSON (the
//! findings ride a `diagnostics` field, never appended text).

use crate::domain::{BuildTool, detect_build_tool};
use async_trait::async_trait;
use regent_kernel::RegentError;
use regent_tools::{ToolCatalog, ToolContext, ToolExecutor};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Hard ceiling per check — a cold `cargo check` may exceed it; that check is
/// skipped (the end-of-run verify still catches everything).
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
/// Error lines kept from the checker's output.
const MAX_LINES: usize = 15;

/// One per harness run: the workspace root and its build tool, detected once.
pub struct Diagnostics {
    workspace: PathBuf,
    tool: Option<BuildTool>,
}

impl Diagnostics {
    /// Detects the workspace's build tool from its root entries. Unreadable
    /// root → no tool → every check is a silent skip.
    #[must_use]
    pub fn detect(workspace: &Path) -> Self {
        let names: Vec<String> = std::fs::read_dir(workspace)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            workspace: workspace.to_owned(),
            tool: detect_build_tool(&names),
        }
    }

    /// The checker argv for `changed`, or `None` when no cheap check applies.
    fn command_for(&self, changed: &str) -> Option<Vec<String>> {
        let ext = Path::new(changed).extension()?.to_str()?;
        let owned = |argv: &[&str]| Some(argv.iter().map(|s| (*s).to_owned()).collect());
        match ext {
            "rs" if self.tool == Some(BuildTool::Cargo) => {
                owned(&["cargo", "check", "-q", "--message-format=short"])
            }
            "ts" | "tsx" if self.workspace.join("tsconfig.json").exists() => {
                owned(&["tsc", "--noEmit"])
            }
            "js" | "mjs" | "cjs" => owned(&["node", "--check", changed]),
            "py" => owned(&["python", "-m", "py_compile", changed]),
            _ => None,
        }
    }

    /// Runs the per-language check for `changed`. `Some(output)` = it reported
    /// errors; `None` = clean, not applicable, or the check itself failed
    /// (spawn error / timeout) — diagnostics must never fail an edit.
    pub async fn check(&self, changed: &str) -> Option<String> {
        let argv = self.command_for(changed)?;
        let run = tokio::process::Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&self.workspace)
            .kill_on_drop(true)
            .output();
        let output = match tokio::time::timeout(CHECK_TIMEOUT, run).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                tracing::debug!(check = argv[0], %error, "diagnostics checker unavailable");
                return None;
            }
            Err(_) => {
                tracing::debug!(
                    check = argv[0],
                    file = changed,
                    "diagnostics check timed out"
                );
                return None;
            }
        };
        if output.status.success() {
            return None;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stderr
            .lines()
            .chain(stdout.lines())
            .filter(|l| !l.trim().is_empty())
            .take(MAX_LINES)
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}

/// How a wrapper finds the changed file in its tool's args.
type PathFrom = fn(&Value) -> Option<String>;

/// Extracts the edited path from `file_edit`/`write_file` args.
fn path_arg(args: &Value) -> Option<String> {
    args.get("path").and_then(Value::as_str).map(str::to_owned)
}

/// Extracts the first touched path from an `apply_patch` V4A patch body.
fn patch_path(args: &Value) -> Option<String> {
    args.get("patch")?.as_str()?.lines().find_map(|line| {
        line.strip_prefix("*** Update File: ")
            .or_else(|| line.strip_prefix("*** Add File: "))
            .map(|p| p.trim().to_owned())
    })
}

/// Decorator: run the inner edit, then attach checker findings (if any) as a
/// `diagnostics` field on the successful JSON-object result.
struct DiagnosticsWrap {
    inner: Arc<dyn ToolExecutor>,
    diagnostics: Arc<Diagnostics>,
    path_from: PathFrom,
}

#[async_trait]
impl ToolExecutor for DiagnosticsWrap {
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, RegentError> {
        let result = self.inner.execute(args.clone(), ctx).await?;
        let Some(path) = (self.path_from)(&args) else {
            return Ok(result);
        };
        // Only a successful edit earns a check — error results pass through.
        let Ok(mut value) = serde_json::from_str::<Value>(&result) else {
            return Ok(result);
        };
        if !value.is_object() || value.get("error").is_some() {
            return Ok(result);
        }
        if let Some(output) = self.diagnostics.check(&path).await {
            value["diagnostics"] = json!({ "file": path, "output": output });
            return Ok(value.to_string());
        }
        Ok(result)
    }
}

/// Wraps the editing tools of `catalog` with edit-time diagnostics for
/// `workspace`. Applied to code-execute catalogs only (chat sessions are
/// untouched); missing tools are silently skipped.
pub fn wrap_diagnostics(catalog: &mut ToolCatalog, workspace: &Path) {
    let diagnostics = Arc::new(Diagnostics::detect(workspace));
    let wraps: [(&str, PathFrom); 3] = [
        ("file_edit", path_arg),
        ("write_file", path_arg),
        ("apply_patch", patch_path),
    ];
    for (name, path_from) in wraps {
        let diagnostics = Arc::clone(&diagnostics);
        catalog.wrap_executor(name, move |inner| {
            Arc::new(DiagnosticsWrap {
                inner,
                diagnostics,
                path_from,
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_path_finds_update_and_add_headers() {
        let patch =
            json!({"patch": "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n*** End Patch"});
        assert_eq!(patch_path(&patch).as_deref(), Some("src/lib.rs"));
        let add = json!({"patch": "*** Begin Patch\n*** Add File: a/b.py\n*** End Patch"});
        assert_eq!(patch_path(&add).as_deref(), Some("a/b.py"));
        assert_eq!(patch_path(&json!({"patch": "no headers"})), None);
    }

    #[test]
    fn command_selection_respects_manifests() {
        let dir = tempfile::tempdir().unwrap();
        // No manifests at all → rust/ts checks unavailable, node/py still apply.
        let d = Diagnostics::detect(dir.path());
        assert_eq!(d.command_for("src/main.rs"), None);
        assert_eq!(d.command_for("app.ts"), None);
        assert!(d.command_for("app.js").is_some());
        assert!(d.command_for("app.py").is_some());
        assert_eq!(d.command_for("README.md"), None);

        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        let d = Diagnostics::detect(dir.path());
        assert_eq!(
            d.command_for("src/main.rs").unwrap()[..2],
            ["cargo".to_owned(), "check".to_owned()]
        );
        assert_eq!(d.command_for("app.ts").unwrap()[0], "tsc");
    }
}
