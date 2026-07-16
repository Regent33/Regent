//! Asking for administrator up front.
//!
//! Owner decision (2026-07-16): Setup and the uninstaller request administrator
//! the moment they start, so an install can go wherever the user points it —
//! `D:\Program Files\Regent` included. This reverses the per-user-only stance
//! docs/BUILDING.md used to take; see docs/adr/ADR-036.
//!
//! Done by relaunching ourselves rather than with an embedded
//! `requestedExecutionLevel=requireAdministrator` manifest. The manifest is the
//! obvious way and it is the wrong one here: NSIS hands off with
//! `nsis_tauri_utils::RunAsUser`, a CreateProcess-family call, and those fail
//! with ERROR_ELEVATION_REQUIRED rather than showing a prompt — the "Run Regent
//! Setup" checkbox would silently do nothing. `tauri dev` starts the binary the
//! same way.
//!
//! Refusing the prompt is deliberately not fatal. A per-user install into
//! %LOCALAPPDATA% never needed administrator, and an installer that cannot run
//! at all for someone without admin rights would be worse than the one this
//! replaces.

/// Marks the elevated copy so it doesn't ask again and fork-bomb the desktop.
/// Internal, unlike the dev-only `--uninstall`.
#[cfg(windows)]
const ELEVATED: &str = "--elevated";

/// Whether this process should carry on. `false` means an elevated copy has
/// taken over and this one should quit without showing a window.
#[cfg(windows)]
pub fn ensure_elevated() -> bool {
    // Dev skips it: `tauri dev` starts the binary with CreateProcess (which
    // cannot elevate) and would prompt on every reload besides.
    if cfg!(debug_assertions) || std::env::args().any(|a| a == ELEVATED) {
        return true;
    }
    let Ok(me) = std::env::current_exe() else {
        return true; // can't relaunch what we can't name — carry on as we are
    };

    // -Verb RunAs *is* the UAC prompt, and an already-elevated caller gets no
    // prompt at all, so there is nothing to test for first. The path travels by
    // environment rather than interpolation: it is not ours to trust as a
    // quoted literal.
    let handed_over = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "Start-Process -FilePath $env:REGENT_SELF -ArgumentList '--elevated' -Verb RunAs",
        ])
        .env("REGENT_SELF", &me)
        .status()
        .is_ok_and(|s| s.success());

    // A refused prompt makes Start-Process throw, so powershell exits non-zero
    // and we simply continue without administrator.
    !handed_over
}

#[cfg(not(windows))]
pub fn ensure_elevated() -> bool {
    true
}
