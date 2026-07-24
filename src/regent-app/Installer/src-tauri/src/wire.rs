//! The bits the install scripts don't do: shortcuts and an uninstall entry.
//! Per-user only — nothing here needs elevation. PATH is left to install.ps1 /
//! install.sh so there is exactly one place that owns it.
//!
//! This file owns the shared boundary and orchestration: the deacon pin, the
//! PowerShell-literal escaper (`ps_lit`) and the `powershell` runner that every
//! Windows side effect flows through, plus `run`. The side effects themselves
//! live in the submodules — `shortcuts` writes the Desktop / Start Menu /
//! `.desktop` entries; `uninstall` registers the uninstaller and reverses
//! everything — so no single file here exceeds the size budget.

#[cfg(windows)]
use crate::log;
use crate::InstallOptions;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

// The `.desktop` file format (writer + parser round trip) is its own focused
// unit, Linux-only — split out so no file here exceeds the size budget.
#[cfg(target_os = "linux")]
mod desktop_entry;
mod shortcuts;
mod uninstall;

// The surface other modules reach as `wire::X`. `run` and `deacon_path` are
// defined here; `unwire` (and, on Linux, `dir_from_desktop_entry`) are the undo
// side, re-exported so callers keep a single import path.
#[cfg(target_os = "linux")]
pub(crate) use desktop_entry::dir_from_desktop_entry;
pub(crate) use uninstall::unwire;

/// The uninstaller is this same binary under another name — `main` routes on it.
/// Copying ourselves keeps one design, one progress UI, and one set of screens
/// instead of a second app to keep in sync. Windows-only, like the GUI
/// uninstaller itself; off Windows this const has no user and `-D warnings`
/// would reject it, so it is gated. Defined here (not in `uninstall`) because
/// `flow` routes on it too, and a re-export used only from tests would read as
/// dead code in a release build.
#[cfg(windows)]
pub(crate) const UNINSTALLER_NAME: &str = "uninstall.exe";

/// The installed desktop app executable.
#[cfg(not(target_os = "macos"))]
fn app_exe(install_dir: &str) -> PathBuf {
    Path::new(install_dir).join("app").join(if cfg!(windows) {
        "Regent.exe"
    } else {
        "Regent"
    })
}

/// The deacon the desktop app must talk to.
pub fn deacon_path(install_dir: &str) -> PathBuf {
    Path::new(install_dir).join("bin").join(if cfg!(windows) {
        "regent-deacon.exe"
    } else {
        "regent-deacon"
    })
}

pub fn run(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    pin_deacon(app, options)?;
    // Always discoverable: the Start Menu entry is what Windows Search indexes,
    // so it goes in regardless of the optional Desktop shortcut. No-op off
    // Windows (Linux's app-menu entry is the `.desktop` shortcut below), so
    // macOS/Linux behavior is unchanged.
    shortcuts::start_menu(app, options)?;
    if options.desktop_shortcut {
        shortcuts::desktop(app, options)?;
    }
    uninstall::entry(app, options)
}

/// Point the desktop app at the deacon explicitly.
///
/// Its `find_deacon()` falls back to PATH, which is wrong for us twice over:
/// PATH is optional (the checkbox), and a PATH written by install.ps1 is not
/// visible to any process that already exists — including the app we launch
/// from the finish screen. A persisted user env var is read by every later
/// launch (shortcut, Start menu) regardless of the PATH choice.
#[cfg(windows)]
fn pin_deacon(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let deacon = deacon_path(&options.install_dir);
    powershell(&format!(
        "[Environment]::SetEnvironmentVariable('REGENT_DEACON_PATH', {}, 'User')",
        ps_lit(&deacon.display().to_string())
    ))?;
    log(app, format!("  deacon: {}", deacon.display()));
    Ok(())
}

/// On Linux the `.desktop` entry carries the variable (see `shortcuts`), and
/// there is no user-wide env store to write to, so this is a no-op.
#[cfg(not(windows))]
fn pin_deacon(_app: &AppHandle, _options: &InstallOptions) -> Result<(), String> {
    Ok(())
}

/// A PowerShell single-quoted literal. Inside '' the only escape is '' itself.
/// The install directory comes from a user-editable text field, so a path like
/// `C:\Users\O'Brien\Regent` would otherwise break out of the quoting — this is
/// the boundary where that gets neutralised. Shared by `pin_deacon`, the
/// shortcut writers, and `unwire`.
#[cfg(windows)]
fn ps_lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(windows)]
fn powershell(script: &str) -> Result<(), String> {
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    // The install path is typed by hand into the Location screen. `ps_lit` is
    // the only thing standing between a name like O'Brien and a registry write
    // or `.lnk` target that means something other than intended.
    #[test]
    #[cfg(windows)]
    fn ps_lit_escapes_quotes() {
        assert_eq!(ps_lit(r"C:\Regent"), r"'C:\Regent'");
        assert_eq!(ps_lit(r"C:\O'Brien\Regent"), r"'C:\O''Brien\Regent'");
        // The classic break-out: close the string, run a command, reopen.
        assert_eq!(
            ps_lit(r"'; Remove-Item C:\ -Recurse; '"),
            r"'''; Remove-Item C:\ -Recurse; '''"
        );
    }

    #[test]
    fn deacon_is_pinned_inside_the_install_dir() {
        // The desktop app resolves the deacon through this path, so it must
        // point at bin/, not at wherever PATH happens to lead.
        let p = deacon_path(r"C:\Regent");
        assert!(p.ends_with(if cfg!(windows) {
            "regent-deacon.exe"
        } else {
            "regent-deacon"
        }));
        assert!(p.parent().unwrap().ends_with("bin"));
    }
}
