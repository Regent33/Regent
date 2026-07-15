//! Uninstall — the mirror of `install` + `wire`, over the same staged UI.
//!
//! This runs from the copy of ourselves that the `wire` stage left in the
//! install directory, so the whole flow (design, screens, progress, log) is the
//! one binary in a second mode rather than a second app to keep in sync.
//!
//! `~/.regent` is never touched. It holds the user's config, keys and memory,
//! and removing the program is not consent to delete their data.

use crate::{log, wire};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// The install directory: we live in it (`<install_dir>/uninstall.exe`).
pub fn install_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate myself: {e}"))?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "uninstaller has no parent directory".to_string())
}

/// Stop anything we are about to delete — a running deacon or app holds its
/// own .exe open, and Windows will not unlink a mapped image.
pub fn stop_processes(app: &AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    for name in ["Regent.exe", "regent-deacon.exe", "regent-cli.exe"] {
        // /F because a hidden stdio child has no window to close politely, and
        // a missing process is a success here, not an error — hence no status
        // check: taskkill exits non-zero simply for "not found".
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", name])
            .output();
    }
    log(app, "  stopped running Regent processes".into());
    Ok(())
}

/// Remove one subdirectory of the install dir.
pub fn remove_dir(app: &AppHandle, dir: &Path, name: &str) -> Result<(), String> {
    let target = dir.join(name);
    if !target.exists() {
        log(app, format!("  {name}/ already gone"));
        return Ok(());
    }
    std::fs::remove_dir_all(&target).map_err(|e| format!("remove {target:?}: {e}"))?;
    log(app, format!("  removed {}", target.display()));
    Ok(())
}

/// Undo the `wire` stage: PATH entry, deacon pin, shortcut, ARP entry.
pub fn unwire(app: &AppHandle, dir: &Path) -> Result<(), String> {
    wire::unwire(app, dir)
}

/// Delete the install directory — including this executable.
///
/// A running .exe cannot delete itself, so the last step is handed to a
/// detached shell that waits for us to exit first. This is the one thing that
/// cannot be verified from inside the process that scheduled it.
pub fn schedule_self_delete(app: &AppHandle, dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        // The path travels by environment variable, not string interpolation:
        // it is user-chosen and would otherwise need quoting inside a quoted
        // -Command string.
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "Start-Sleep 2; Remove-Item -Recurse -Force -LiteralPath $env:REGENT_UNINSTALL_DIR",
            ])
            .env("REGENT_UNINSTALL_DIR", dir)
            .spawn()
            .map_err(|e| format!("cannot schedule cleanup: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        // No self-delete problem worth solving here: unlinking a running binary
        // is legal on POSIX, the inode survives until exit.
        std::fs::remove_dir_all(dir).map_err(|e| format!("remove {dir:?}: {e}"))?;
    }
    log(app, format!("  removing {}", dir.display()));
    Ok(())
}
