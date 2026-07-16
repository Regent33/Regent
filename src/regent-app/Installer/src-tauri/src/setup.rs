//! Discarding Setup's own files once they have served their purpose.
//!
//! NSIS unpacks `regent-installer.exe` and the payload into a directory of their
//! own and registers "Regent Setup" in Apps & features — it treats Setup as an
//! application you install, when it is a thing you run once. Left alone that
//! parks ~68MB and a second Apps & features row forever, beside the real
//! "Regent" the install just created.
//!
//! NSIS already wrote a complete uninstaller for its own artifacts, so this
//! runs that rather than re-deleting its files and registry keys by hand.
//! Windows-only by nature: there is no unpacked directory to discard when the
//! same app ships as a .dmg or an AppImage.

// Windows-only imports: every other platform's `discard` is an empty stub, and
// an unused import is a warning there.
#[cfg(windows)]
use crate::log;
#[cfg(windows)]
use std::path::Path;
use tauri::AppHandle;

/// Hand Setup's directory to NSIS's own silent uninstaller, once we exit.
///
/// Never fails the install: the payload having outlived its usefulness is
/// untidy, not broken, and by the time this runs Regent is already in place.
///
/// Guarded, because getting it wrong deletes the wrong directory:
/// * the Apps & features entry's `InstallLocation` must be *this* directory, so
///   a `cargo run` out of `target/release` matches nothing and does nothing;
/// * the directory we just installed into is never a candidate;
/// * the cleanup waits on this process rather than sleeping a guessed interval
///   — NSIS refuses to uninstall while `regent-installer.exe` is still alive.
#[cfg(windows)]
pub fn discard(app: &AppHandle, install_dir: &str) {
    let Ok(me) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = me.parent() else {
        return;
    };

    // Setup unpacked into the very directory the user chose to install into
    // would mean NSIS's uninstaller — which removes its whole INSTDIR — taking
    // the fresh install down with it.
    if same_dir(dir, Path::new(install_dir)) {
        return;
    }

    // The logic lives in the script so there is no registry output to parse
    // back: find the Apps & features entry whose InstallLocation is this
    // directory (NSIS writes it quoted), wait for us to go, then run the
    // uninstaller NSIS wrote for its own files.
    //
    // The containment check on $exe is a security control, not a sanity one. We
    // run elevated, and HKCU is writable by any code running as this user — so
    // an UninstallString taken on faith is an invitation to plant one and be
    // handed administrator the moment the UAC prompt is accepted. Only a path
    // inside the directory we are already deleting is worth running.
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$dir = ($env:REGENT_SETUP_DIR).TrimEnd('\')
$root = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
$entry = Get-ChildItem $root | Where-Object {
  ((Get-ItemProperty $_.PSPath).InstallLocation -replace '"', '').TrimEnd('\') -ieq $dir
} | Select-Object -First 1
if (-not $entry) { exit }
$exe = ((Get-ItemProperty $entry.PSPath).UninstallString -replace '"', '')
if (-not $exe.StartsWith($dir + '\', [StringComparison]::OrdinalIgnoreCase)) { exit }
if (-not (Test-Path -LiteralPath $exe)) { exit }
Wait-Process -Id $env:REGENT_SETUP_PID
Start-Process -FilePath $exe -ArgumentList '/S' -Wait
"#;

    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        // Somewhere else to stand. A child inherits our working directory —
        // the Setup directory it is here to delete — and nothing can remove its
        // own cwd, so NSIS's RMDir would leave the folder behind. Verified: an
        // empty Setup directory survived every run before this line existed.
        .current_dir(std::env::temp_dir())
        // Both travel as environment rather than interpolated into the script:
        // the directory is a path we did not choose and would otherwise need
        // quoting inside an already-quoted -Command string.
        .env("REGENT_SETUP_DIR", dir)
        .env("REGENT_SETUP_PID", std::process::id().to_string())
        .spawn()
    {
        Ok(_) => log(app, "  Setup files will be removed on exit".into()),
        Err(e) => log(app, format!("  (leaving Setup files behind: {e})")),
    }
}

#[cfg(not(windows))]
pub fn discard(_app: &AppHandle, _install_dir: &str) {}

/// Windows paths are case-insensitive, and the install directory is typed by
/// hand — `C:\X\` and `c:/x` are one place.
#[cfg(windows)]
fn same_dir(a: &Path, b: &Path) -> bool {
    fn norm(p: &Path) -> String {
        p.canonicalize()
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .to_lowercase()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string()
    }
    norm(a) == norm(b)
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;

    // The one guard with catastrophic consequences: matching here means we
    // hand the directory holding the brand-new install to a recursive delete.
    #[test]
    fn same_dir_ignores_case_and_trailing_separators() {
        assert!(same_dir(Path::new(r"C:\Regent"), Path::new(r"c:\regent\")));
        assert!(same_dir(Path::new(r"C:\Regent\"), Path::new("C:/Regent")));
        assert!(!same_dir(
            Path::new(r"C:\Regent Setup"),
            Path::new(r"C:\Regent")
        ));
        // The real pairing: Setup's own dir vs. the default install target.
        assert!(!same_dir(
            Path::new(r"C:\Users\x\AppData\Local\Regent Setup"),
            Path::new(r"C:\Users\x\AppData\Local\Programs\Regent")
        ));
    }
}
