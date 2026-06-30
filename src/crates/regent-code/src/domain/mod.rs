//! Pure coding-harness decisions — no I/O, no framework imports. Three things:
//! detect the repo's verify lane from its manifests, choose the plan-mode tool
//! subset, and read a command's exit/output into a pass/fail outcome.

/// The verify lane detected for a repo — the test/build command run after an
/// edit batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTool {
    Cargo,
    Npm,
    Make,
    Pytest,
}

impl BuildTool {
    /// The argv this lane runs to verify (program first). Pure mapping; the
    /// infra runner spawns it.
    #[must_use]
    pub fn verify_command(self) -> &'static [&'static str] {
        match self {
            BuildTool::Cargo => &["cargo", "test"],
            BuildTool::Npm => &["npm", "test"],
            BuildTool::Make => &["make", "test"],
            BuildTool::Pytest => &["pytest"],
        }
    }
}

/// Returns the file name of `path` (after the last `/` or `\`).
fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn has(files: &[String], manifest: &str) -> bool {
    files.iter().any(|f| basename(f) == manifest)
}

/// Detects the verify lane from a list of files (typically the repo root's
/// entries). Precedence is **Cargo > Npm > Pytest > Make**: a language-native
/// runner is more specific than a generic `Makefile`, which often just wraps it.
/// Returns `None` when no known manifest is present (verify is then skipped).
#[must_use]
pub fn detect_build_tool(files: &[String]) -> Option<BuildTool> {
    if has(files, "Cargo.toml") {
        return Some(BuildTool::Cargo);
    }
    if has(files, "package.json") {
        return Some(BuildTool::Npm);
    }
    if has(files, "pyproject.toml")
        || has(files, "setup.py")
        || has(files, "pytest.ini")
        || has(files, "tox.ini")
    {
        return Some(BuildTool::Pytest);
    }
    if has(files, "Makefile") {
        return Some(BuildTool::Make);
    }
    None
}

/// Plan-mode state. Phase 1 is read-only (understand + produce a plan); phase 2
/// enables the full toolset and executes the approved plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Plan,
    Execute,
}

/// Tools allowed in plan-mode: read-only only — never write, edit, or run
/// terminal/mutating commands. Plan phase is for understanding, not changing.
const PLAN_READ_ONLY: &[&str] = &["read_file", "glob", "search_files", "ls"];

/// The tool subset for `phase`, derived from the agent's `full` toolset.
/// `Execute` keeps everything; `Plan` keeps only the read-only allowlist that is
/// actually present (so a missing read tool isn't conjured, and a write tool
/// can never leak into plan-mode).
#[must_use]
pub fn plan_toolset(phase: Phase, full: &[String]) -> Vec<String> {
    match phase {
        Phase::Execute => full.to_vec(),
        Phase::Plan => full
            .iter()
            .filter(|t| PLAN_READ_ONLY.contains(&t.as_str()))
            .cloned()
            .collect(),
    }
}

/// The parsed result of running a verify command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyOutcome {
    pub passed: bool,
    pub summary: String,
}

/// Reads a finished verify command into a pass/fail outcome. `passed` is exit
/// code 0 (a signal-killed process — `None` — is a failure). `summary` is the
/// last non-empty output line: stdout's on success (test runners print the
/// "ok / N passed" line there), stderr's on failure, falling back across the
/// two so the result is never blank.
#[must_use]
pub fn parse_verify(exit_code: Option<i32>, stdout: &str, stderr: &str) -> VerifyOutcome {
    let passed = exit_code == Some(0);
    let last_line = |s: &str| {
        s.lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_owned())
            .unwrap_or_default()
    };
    let summary = if passed {
        let s = last_line(stdout);
        if s.is_empty() {
            "verification passed".to_owned()
        } else {
            s
        }
    } else {
        match (last_line(stderr), last_line(stdout)) {
            (e, _) if !e.is_empty() => e,
            (_, o) if !o.is_empty() => o,
            _ => match exit_code {
                Some(c) => format!("verification failed (exit {c})"),
                None => "verification failed (terminated by signal)".to_owned(),
            },
        }
    };
    VerifyOutcome { passed, summary }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn detect_table() {
        assert_eq!(
            detect_build_tool(&owned(&["Cargo.toml"])),
            Some(BuildTool::Cargo)
        );
        assert_eq!(
            detect_build_tool(&owned(&["package.json"])),
            Some(BuildTool::Npm)
        );
        assert_eq!(
            detect_build_tool(&owned(&["pyproject.toml"])),
            Some(BuildTool::Pytest)
        );
        assert_eq!(
            detect_build_tool(&owned(&["setup.py"])),
            Some(BuildTool::Pytest)
        );
        assert_eq!(
            detect_build_tool(&owned(&["pytest.ini"])),
            Some(BuildTool::Pytest)
        );
        assert_eq!(
            detect_build_tool(&owned(&["tox.ini"])),
            Some(BuildTool::Pytest)
        );
        assert_eq!(
            detect_build_tool(&owned(&["Makefile"])),
            Some(BuildTool::Make)
        );
        assert_eq!(detect_build_tool(&owned(&["README.md", "src"])), None);
        assert_eq!(detect_build_tool(&[]), None);
    }

    #[test]
    fn detect_precedence_and_basename() {
        // Cargo wins over a co-present package.json (a Rust repo with tooling).
        assert_eq!(
            detect_build_tool(&owned(&["package.json", "Cargo.toml", "Makefile"])),
            Some(BuildTool::Cargo)
        );
        // Pytest beats a wrapping Makefile.
        assert_eq!(
            detect_build_tool(&owned(&["Makefile", "pyproject.toml"])),
            Some(BuildTool::Pytest)
        );
        // Matches on the file name even when given full paths.
        assert_eq!(
            detect_build_tool(&owned(&["repo/sub/Cargo.toml"])),
            Some(BuildTool::Cargo)
        );
        // A look-alike name does not match (basename equality, not suffix).
        assert_eq!(detect_build_tool(&owned(&["NotCargo.toml"])), None);
    }

    #[test]
    fn verify_commands() {
        assert_eq!(BuildTool::Cargo.verify_command(), &["cargo", "test"]);
        assert_eq!(BuildTool::Npm.verify_command(), &["npm", "test"]);
        assert_eq!(BuildTool::Make.verify_command(), &["make", "test"]);
        assert_eq!(BuildTool::Pytest.verify_command(), &["pytest"]);
    }

    #[test]
    fn plan_subset_keeps_only_present_read_only_tools() {
        let full = owned(&[
            "read_file",
            "write_file",
            "file_edit",
            "search_files",
            "glob",
            "ls",
            "terminal",
        ]);
        let mut plan = plan_toolset(Phase::Plan, &full);
        plan.sort();
        assert_eq!(plan, owned(&["glob", "ls", "read_file", "search_files"]));
        // Execute keeps the full set, order preserved.
        assert_eq!(plan_toolset(Phase::Execute, &full), full);
    }

    #[test]
    fn plan_subset_does_not_conjure_absent_tools() {
        // Only `ls` of the read-only set is present → only `ls` survives.
        let plan = plan_toolset(Phase::Plan, &owned(&["ls", "terminal", "write_file"]));
        assert_eq!(plan, owned(&["ls"]));
    }

    #[test]
    fn verify_pass_uses_stdout_tail() {
        let out = parse_verify(
            Some(0),
            "compiling\ntest result: ok. 5 passed; 0 failed\n",
            "",
        );
        assert!(out.passed);
        assert_eq!(out.summary, "test result: ok. 5 passed; 0 failed");
    }

    #[test]
    fn verify_fail_uses_stderr_tail() {
        let out = parse_verify(
            Some(101),
            "running 3 tests\n",
            "error[E0433]: failed to resolve\n",
        );
        assert!(!out.passed);
        assert_eq!(out.summary, "error[E0433]: failed to resolve");
    }

    #[test]
    fn verify_fail_falls_back_to_stdout_then_exit_code() {
        // Failure with no stderr → stdout tail.
        let out = parse_verify(Some(1), "FAILED tests/test_x.py::test_y\n", "");
        assert!(!out.passed);
        assert_eq!(out.summary, "FAILED tests/test_x.py::test_y");
        // Failure with no output at all → exit-code summary.
        let out = parse_verify(Some(2), "  \n", "\n");
        assert!(!out.passed);
        assert_eq!(out.summary, "verification failed (exit 2)");
        // Signal-killed (None) is a failure, never a pass.
        let out = parse_verify(None, "", "");
        assert!(!out.passed);
        assert_eq!(out.summary, "verification failed (terminated by signal)");
    }
}
