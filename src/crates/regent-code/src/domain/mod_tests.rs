//! Unit tests for `mod` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

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
