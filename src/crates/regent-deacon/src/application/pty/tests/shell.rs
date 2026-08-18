//! Shell resolution, driven on one machine by passing the environment in.

use super::*;

/// Nothing is on PATH — forces every fallback to be exercised.
fn none(_: &str) -> bool {
    false
}

/// Everything is on PATH.
fn all(_: &str) -> bool {
    true
}

#[test]
fn the_override_wins_on_every_platform() {
    for windows in [true, false] {
        assert_eq!(
            choose(Some("nu"), Some("/bin/bash"), windows, &none),
            "nu",
            "REGENT_PTY_SHELL must beat the platform default"
        );
    }
}

/// A blank env var is not a choice. `SHELL=` shows up in stripped environments
/// (cron, some containers) and honouring it literally would spawn `""`.
#[test]
fn blank_env_vars_are_treated_as_absent() {
    assert_eq!(
        choose(Some("   "), Some("/bin/zsh"), false, &none),
        "/bin/zsh"
    );
    assert_eq!(choose(None, Some(""), false, &none), "sh");
    assert_eq!(choose(Some(""), None, true, &none), "cmd.exe");
}

#[test]
fn windows_prefers_powershell_then_falls_back_to_comspec() {
    // Owner's call 2026-07-30: PowerShell, matching VS Code.
    assert_eq!(
        choose(None, Some("C:\\WINDOWS\\cmd.exe"), true, &all),
        "pwsh"
    );

    // pwsh missing, powershell present.
    let only_ps = |p: &str| p == "powershell.exe";
    assert_eq!(choose(None, None, true, &only_ps), "powershell.exe");

    // Neither present: COMSPEC, else cmd.exe. A terminal that opens beats none.
    assert_eq!(
        choose(None, Some("C:\\WINDOWS\\system32\\cmd.exe"), true, &none),
        "C:\\WINDOWS\\system32\\cmd.exe"
    );
    assert_eq!(choose(None, None, true, &none), "cmd.exe");
}

#[test]
fn unix_uses_the_login_shell_before_guessing() {
    // $SHELL is the OS's own record of the user's choice — never second-guess it.
    assert_eq!(choose(None, Some("/bin/fish"), false, &all), "/bin/fish");
    // Missing $SHELL: zsh (macOS default since Catalina), then bash, then sh.
    assert_eq!(choose(None, None, false, &all), "zsh");
    let only_bash = |p: &str| p == "bash";
    assert_eq!(choose(None, None, false, &only_bash), "bash");
    assert_eq!(choose(None, None, false, &none), "sh");
}

/// Whatever this machine is, the real resolver must name something non-empty —
/// an empty program string is a spawn error, not a fallback.
#[test]
fn resolve_names_a_shell_on_this_machine() {
    let shell = resolve();
    assert!(!shell.trim().is_empty(), "resolved an empty shell");
}

#[test]
fn windows_turns_a_bare_shell_name_into_the_executable_where_found() {
    let found = |program: &str| Some(format!("C:\\Tools\\{program}.exe"));
    assert_eq!(
        resolve_windows_program("pwsh".into(), true, &found),
        "C:\\Tools\\pwsh.exe"
    );

    // An explicit path is returned untouched — and "explicit" must be decided
    // by WINDOWS rules, not the host's. `std::path::Path` reported one
    // component for this on Linux, so CI searched a path the user had spelled
    // out. Every spelling a person actually types is covered here.
    for explicit in [
        "C:\\Portable\\nu.exe",
        "C:/Portable/nu.exe",
        ".\\nu.exe",
        "\\\\server\\share\\nu.exe",
        "C:nu.exe",
    ] {
        assert_eq!(
            resolve_windows_program(explicit.into(), true, &|_| panic!("must not search")),
            explicit
        );
    }
    assert_eq!(
        resolve_windows_program("bash".into(), false, &|_| panic!("must not search")),
        "bash"
    );
}
