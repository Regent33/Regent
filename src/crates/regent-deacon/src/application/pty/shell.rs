//! Which shell an interactive terminal launches, per platform.
//!
//! Pure and separated from the PTY plumbing so the per-platform order is
//! testable without spawning anything — the wrong shell here is a terminal that
//! opens and immediately dies, and that deserves a test rather than a hope.
//!
//! This deliberately does NOT match `regent-tools`' `LocalBackend`, which runs
//! `cmd /C` on Windows. That is a non-interactive command runner, and it uses
//! cmd for a specific escaping reason (Rust's default `\"` quoting corrupts a
//! command like `start "" "https://…"`). A human's interactive shell is a
//! different question, and the owner's answer was PowerShell (2026-07-30),
//! matching the editor people arrive from.

/// Env var that overrides everything — an escape hatch for anyone whose shell
/// is not on the list, and the seam the tests drive.
pub const SHELL_OVERRIDE_ENV: &str = "REGENT_PTY_SHELL";

/// Resolves the shell program to launch.
///
/// `override_var` is `$REGENT_PTY_SHELL`, `platform_var` is `%COMSPEC%` on
/// Windows and `$SHELL` elsewhere, and `has` reports whether a program exists on
/// PATH. Taking all three as arguments is what makes the decision testable on
/// one machine: the real lookups happen in [`resolve`].
///
/// Blank env vars are treated as absent. An empty `SHELL=` is common in stripped
/// environments (cron, some containers) and honouring it literally would try to
/// spawn `""`.
pub fn choose(
    override_var: Option<&str>,
    platform_var: Option<&str>,
    windows: bool,
    has: &dyn Fn(&str) -> bool,
) -> String {
    if let Some(explicit) = override_var.map(str::trim).filter(|v| !v.is_empty()) {
        return explicit.to_owned();
    }
    if windows {
        // PowerShell 7+ first, then the one every Windows install ships, then
        // whatever COMSPEC names (cmd.exe in practice) as the last resort — a
        // terminal that opens is better than no terminal.
        for candidate in ["pwsh", "powershell.exe"] {
            if has(candidate) {
                return candidate.to_owned();
            }
        }
        return platform_var
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("cmd.exe")
            .to_owned();
    }
    // Unix: the user's own login shell is the right answer and the OS already
    // records it. Only guess when it is missing.
    if let Some(shell) = platform_var.map(str::trim).filter(|v| !v.is_empty()) {
        return shell.to_owned();
    }
    for candidate in ["zsh", "bash"] {
        if has(candidate) {
            return candidate.to_owned();
        }
    }
    "sh".to_owned()
}

/// [`choose`] against the real environment and PATH.
#[must_use]
pub fn resolve() -> String {
    let over = std::env::var(SHELL_OVERRIDE_ENV).ok();
    let platform = std::env::var(if cfg!(windows) { "COMSPEC" } else { "SHELL" }).ok();
    let selected = choose(
        over.as_deref(),
        platform.as_deref(),
        cfg!(windows),
        &on_path,
    );
    resolve_windows_program(selected, cfg!(windows), &find_on_path)
}

/// ConPTY's process launcher does not reliably resolve a bare program name
/// through PATH (portable-pty can hand `CreateProcessW` a literal `pwsh\0`).
/// Resolve it before building the command. Explicit paths remain untouched, so
/// `REGENT_PTY_SHELL=C:\\tools\\nu.exe` still means exactly what the user wrote.
fn resolve_windows_program(
    selected: String,
    windows: bool,
    find: &dyn Fn(&str) -> Option<String>,
) -> String {
    if !windows || std::path::Path::new(&selected).components().count() > 1 {
        return selected;
    }
    find(&selected).unwrap_or(selected)
}

/// Whether `program` is runnable. `which`/`where` rather than a PATH walk: they
/// already know about PATHEXT on Windows and about shell builtins' locations.
fn on_path(program: &str) -> bool {
    find_on_path(program).is_some()
}

fn find_on_path(program: &str) -> Option<String> {
    let finder = if cfg!(windows) { "where" } else { "which" };
    let output = std::process::Command::new(finder)
        .arg(program)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
#[path = "tests/shell.rs"]
mod tests;
