//! Desktop / Start Menu / application-menu shortcuts, per platform.
//!
//! Windows gets two: a Start Menu entry (always — it is what Windows Search
//! indexes, so typing "Regent" finds the app) and an optional Desktop shortcut.
//! Linux writes a single `.desktop` entry that is both the app-menu shortcut
//! and the breadcrumb Setup reads back to find a custom install on uninstall.
//! macOS relies on Spotlight.

#[cfg(windows)]
use super::app_exe;
use crate::{log, InstallOptions};
#[cfg(any(windows, target_os = "linux"))]
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
use tauri::AppHandle;

// ---- Windows -------------------------------------------------------------

/// The per-user Start Menu shortcut path. Windows Search indexes this folder,
/// so a `.lnk` here is what makes typing "Regent" find the app — independently
/// of the optional Desktop shortcut. Split out as a pure function so the writer
/// (`start_menu`) and the remover (`uninstall::unwire`) build the same path and
/// cannot drift apart unnoticed.
#[cfg(windows)]
pub(crate) fn start_menu_lnk(appdata: &str) -> PathBuf {
    Path::new(appdata)
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join("Regent.lnk")
}

/// Where Windows actually keeps this user's Desktop.
///
/// **Not** `%USERPROFILE%\Desktop`. OneDrive's Known Folder Move redirects the
/// Desktop to `…\OneDrive\Desktop` and does not leave the original behind, so
/// on any machine with OneDrive backup enabled — the default when signing in
/// with a Microsoft account — the composed path does not exist and
/// `IWshShortcut.Save()` throws `DirectoryNotFoundException`. Reported from a
/// fresh install on 2026-08-17, where every binary landed correctly and only
/// this shortcut failed. The Start Menu entry survived the same machine
/// because its folder is never redirected *and* `start_menu` creates it first.
///
/// `GetFolderPath` reads the redirection, so it is the answer rather than a
/// workaround. `%USERPROFILE%\Desktop` remains as a fallback for the case
/// where it returns empty, and a directory that does not exist yields `None`
/// rather than a path that is going to throw — the caller treats that as "this
/// user has no Desktop to put a shortcut on", which is a fact, not an error.
#[cfg(windows)]
pub(crate) fn desktop_dir() -> Option<PathBuf> {
    if let Ok(resolved) = super::powershell_out("[Environment]::GetFolderPath('Desktop')") {
        let path = PathBuf::from(resolved);
        if path.is_dir() {
            return Some(path);
        }
    }
    let fallback = PathBuf::from(std::env::var("USERPROFILE").ok()?).join("Desktop");
    fallback.is_dir().then_some(fallback)
}

/// The Desktop shortcut path within that folder. Same writer/remover
/// drift-proofing as `start_menu_lnk` — both go through `desktop_dir` too, so
/// a redirected Desktop cannot leave the uninstaller looking in the old place.
#[cfg(windows)]
pub(crate) fn desktop_lnk(desktop: &Path) -> PathBuf {
    desktop.join("Regent.lnk")
}

/// Write a `.lnk` with WScript.Shell — the zero-dependency way, versus pulling
/// in the whole COM crate stack for one call. The install directory comes from
/// a user-editable text field, so every argument crosses `ps_lit` first: a path
/// like `C:\Users\O'Brien\Regent` would otherwise break out of the quoting.
/// This is the single escaped path-construction both shortcuts share.
#[cfg(windows)]
fn write_lnk(target: &Path, exe: &Path, working_dir: &Path) -> Result<(), String> {
    super::powershell(&format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({}); \
         $s.TargetPath = {}; $s.WorkingDirectory = {}; $s.Save()",
        super::ps_lit(&target.display().to_string()),
        super::ps_lit(&exe.display().to_string()),
        super::ps_lit(&working_dir.display().to_string()),
    ))
}

/// Always written, regardless of the Desktop-shortcut checkbox: the Start Menu
/// entry is Windows Search's index, and an app you cannot find is one you
/// cannot open.
#[cfg(windows)]
pub(super) fn start_menu(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let appdata = std::env::var("APPDATA")
        .map_err(|_| "no APPDATA — cannot find the Start Menu".to_string())?;
    let lnk = start_menu_lnk(&appdata);
    if let Some(parent) = lnk.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {parent:?}: {e}"))?;
    }
    write_lnk(
        &lnk,
        &app_exe(&options.install_dir),
        &Path::new(&options.install_dir).join("app"),
    )?;
    log(app, format!("  start menu: {}", lnk.display()));
    Ok(())
}

#[cfg(windows)]
pub(super) fn desktop(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let dir = desktop_dir().ok_or_else(|| "this user has no Desktop folder".to_string())?;
    let lnk = desktop_lnk(&dir);
    write_lnk(
        &lnk,
        &app_exe(&options.install_dir),
        &Path::new(&options.install_dir).join("app"),
    )?;
    log(app, format!("  shortcut: {}", lnk.display()));
    Ok(())
}

// ---- non-Windows Start Menu (no-op) --------------------------------------

/// Linux's application-menu entry is the `.desktop` shortcut `desktop` writes
/// (gated on the checkbox, historical behavior); macOS uses Spotlight. Neither
/// has a separate always-on Start Menu to populate, so this is a no-op — Linux
/// and macOS behavior is unchanged.
#[cfg(not(windows))]
pub(super) fn start_menu(_app: &AppHandle, _options: &InstallOptions) -> Result<(), String> {
    Ok(())
}

// ---- Linux ---------------------------------------------------------------

/// Linux's app-menu entry doubles as the uninstall breadcrumb, so the `.desktop`
/// format (writer + parser) lives in its own `desktop_entry` module; this just
/// places the file.
#[cfg(target_os = "linux")]
pub(super) fn desktop(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "no HOME".to_string())?;
    let dir = Path::new(&home).join(".local/share/applications");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {dir:?}: {e}"))?;
    let entry = dir.join("regent.desktop");
    std::fs::write(&entry, super::desktop_entry::render(&options.install_dir))
        .map_err(|e| format!("write {entry:?}: {e}"))?;
    log(app, format!("  shortcut: {}", entry.display()));
    Ok(())
}

// ---- macOS ---------------------------------------------------------------

#[cfg(target_os = "macos")]
pub(super) fn desktop(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    // macOS has no desktop-shortcut convention worth honouring; an alias to the
    // executable is the closest thing, and Spotlight already finds it.
    let _ = options;
    log(
        app,
        "  (no desktop shortcut on macOS — skipped)".to_string(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    // The install path is typed by hand into the Location screen; every
    // shortcut path is built from it. These builders are what the writer and
    // the uninstaller both go through, so they are pinned together.
    #[test]
    #[cfg(windows)]
    fn shortcut_paths_land_where_windows_looks() {
        let sm = start_menu_lnk(r"C:\Users\me\AppData\Roaming");
        assert!(sm.ends_with("Regent.lnk"));
        assert!(
            sm.to_string_lossy().contains(r"Start Menu\Programs"),
            "Start Menu shortcut must sit in the Search-indexed Programs folder"
        );
        // A REDIRECTED desktop, because that is the case that broke: the
        // shortcut must land wherever the folder actually is, not under
        // %USERPROFILE%. Asserting `ends_with("Desktop")` on a composed path
        // was satisfied by the exact bug it was meant to catch.
        let dt = desktop_lnk(Path::new(r"C:\Users\me\OneDrive\Desktop"));
        assert_eq!(
            dt,
            PathBuf::from(r"C:\Users\me\OneDrive\Desktop\Regent.lnk")
        );
    }

    /// The resolver must return a directory that EXISTS, or `None`. Returning
    /// a plausible-looking path that does not exist is what turned a missing
    /// folder into a `DirectoryNotFoundException` at `Save()` — one screen
    /// saying "Something went wrong" after a wholly successful install.
    #[test]
    #[cfg(windows)]
    fn the_resolved_desktop_is_a_real_directory_or_nothing() {
        if let Some(dir) = desktop_dir() {
            assert!(dir.is_dir(), "{dir:?} was handed back but does not exist");
        }
    }

    /// The writer, for real: COM, a `.lnk` on disk, and the target read back.
    ///
    /// Everything above this is path arithmetic, which is what let the original
    /// defect through — the paths looked right and nothing ever tried to save
    /// one. This writes into a directory that exists (the fix's precondition)
    /// and then, in the same test, into one that does not, which is precisely
    /// the redirected-Desktop case and must fail rather than appear to work.
    #[test]
    #[cfg(windows)]
    fn a_shortcut_is_written_into_a_real_folder_and_refused_by_a_missing_one() {
        let base = std::env::temp_dir().join(format!("regent-lnk-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("temp dir");
        let exe = PathBuf::from(r"C:\Windows\System32\notepad.exe");

        let lnk = desktop_lnk(&base);
        write_lnk(&lnk, &exe, &base).expect("a real folder takes a shortcut");
        assert!(lnk.is_file(), "no .lnk at {lnk:?}");
        let target = super::super::powershell_out(&format!(
            "(New-Object -ComObject WScript.Shell).CreateShortcut({}).TargetPath",
            super::super::ps_lit(&lnk.display().to_string())
        ))
        .expect("read the shortcut back");
        assert_eq!(
            PathBuf::from(target),
            exe,
            "the .lnk exists but does not point at the app"
        );

        // The reported failure, reproduced through this same writer.
        let missing = base.join("OneDrive-moved-this");
        assert!(
            write_lnk(&desktop_lnk(&missing), &exe, &base).is_err(),
            "saving into a folder that does not exist must fail, not silently pass"
        );

        std::fs::remove_dir_all(&base).ok();
    }
}
