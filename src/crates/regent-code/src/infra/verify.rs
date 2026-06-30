//! Spawns the repo's detected verify lane and reads its result. Detection +
//! result-parsing are the pure `domain` functions; this file only does the I/O
//! (list the root, run the command).

use crate::application::Verifier;
use crate::domain::{VerifyOutcome, detect_build_tool, parse_verify};
use async_trait::async_trait;
use regent_kernel::RegentError;
use std::path::Path;
use tokio::process::Command;

/// Runs the verify command detected from the workspace root's manifests.
pub struct VerifyRunner;

fn verify_err(message: impl Into<String>) -> RegentError {
    RegentError::Tool {
        tool: "verify".into(),
        message: message.into(),
    }
}

/// Immediate entry names of `dir` (non-recursive) — what `detect_build_tool`
/// matches manifests against.
fn root_entry_names(dir: &Path) -> Result<Vec<String>, RegentError> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| verify_err(e.to_string()))? {
        let entry = entry.map_err(|e| verify_err(e.to_string()))?;
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_owned());
        }
    }
    Ok(names)
}

#[async_trait]
impl Verifier for VerifyRunner {
    async fn verify(&self, workspace: &Path) -> Result<Option<VerifyOutcome>, RegentError> {
        let Some(tool) = detect_build_tool(&root_entry_names(workspace)?) else {
            return Ok(None);
        };
        let argv = tool.verify_command();
        let output = Command::new(argv[0])
            .args(&argv[1..])
            .current_dir(workspace)
            .output()
            .await
            .map_err(|e| verify_err(format!("could not run `{}`: {e}", argv.join(" "))))?;

        let outcome = parse_verify(
            output.status.code(),
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
        );
        tracing::info!(
            tool = ?tool,
            passed = outcome.passed,
            "verify lane complete"
        );
        Ok(Some(outcome))
    }
}
