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

/// The per-user Desktop shortcut path. Same writer/remover drift-proofing as
/// `start_menu_lnk`.
#[cfg(windows)]
pub(crate) fn desktop_lnk(userprofile: &str) -> PathBuf {
    Path::new(userprofile).join("Desktop").join("Regent.lnk")
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
    let profile = std::env::var("USERPROFILE")
        .map_err(|_| "no USERPROFILE — cannot find the desktop".to_string())?;
    let lnk = desktop_lnk(&profile);
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
        let dt = desktop_lnk(r"C:\Users\me");
        assert!(dt.ends_with("Regent.lnk"));
        assert!(dt.parent().unwrap().ends_with("Desktop"));
    }
}
