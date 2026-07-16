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
/// detached shell that waits for us to exit first.
pub fn schedule_self_delete(app: &AppHandle, dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Wait on our own PID rather than sleeping a guessed interval. This is
        // scheduled while the "Removed" screen is still up, so uninstall.exe is
        // still open — and Windows will not unlink a mapped image. A fixed
        // `Start-Sleep 2` therefore failed on every run a person was actually
        // driving, silently orphaning this 9.4MB executable and its directory;
        // it could only have worked if the window were closed within 2 seconds.
        //
        // The retry loop covers the gap between the process exiting and the
        // image being unmapped, which is not instantaneous.
        //
        // Both values travel by environment variable, not string interpolation:
        // the path is user-chosen and would otherwise need quoting inside an
        // already-quoted -Command string.
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "$ErrorActionPreference = 'SilentlyContinue'; \
                 Wait-Process -Id $env:REGENT_UNINSTALL_PID; \
                 for ($i = 0; $i -lt 20; $i++) { \
                   Remove-Item -Recurse -Force -LiteralPath $env:REGENT_UNINSTALL_DIR; \
                   if (-not (Test-Path -LiteralPath $env:REGENT_UNINSTALL_DIR)) { break }; \
                   Start-Sleep -Milliseconds 250 \
                 }",
            ])
            // Somewhere else to stand. A child inherits our working directory,
            // which is the directory it has to delete — and nothing can remove
            // its own cwd, so the contents go and the folder stays. Verified:
            // this is what left an empty install directory behind.
            .current_dir(std::env::temp_dir())
            .env("REGENT_UNINSTALL_DIR", dir)
            .env("REGENT_UNINSTALL_PID", std::process::id().to_string())
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
