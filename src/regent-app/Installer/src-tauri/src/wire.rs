//! The bits the install scripts don't do: a shortcut and an uninstall entry.
//! Per-user only — nothing here needs elevation. PATH is left to install.ps1 /
//! install.sh so there is exactly one place that owns it.

use crate::{log, InstallOptions};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

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
    if options.desktop_shortcut {
        shortcut(app, options)?;
    }
    uninstall_entry(app, options)
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

/// On Linux the .desktop entry carries the variable (see `shortcut`), and there
/// is no user-wide env store to write to, so this is a no-op.
#[cfg(not(windows))]
fn pin_deacon(_app: &AppHandle, _options: &InstallOptions) -> Result<(), String> {
    Ok(())
}

/// A PowerShell single-quoted literal. Inside '' the only escape is '' itself.
/// The install directory comes from a user-editable text field, so a path like
/// `C:\Users\O'Brien\Regent` would otherwise break out of the quoting — this is
/// the boundary where that gets neutralised.
#[cfg(windows)]
fn ps_lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(windows)]
fn shortcut(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let desktop = std::env::var("USERPROFILE")
        .map(|p| Path::new(&p).join("Desktop").join("Regent.lnk"))
        .map_err(|_| "no USERPROFILE — cannot find the desktop".to_string())?;
    let exe = app_exe(&options.install_dir);
    // WScript.Shell is the zero-dependency way to write a .lnk; the alternative
    // is pulling in the whole COM crate stack for one call.
    powershell(&format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({}); \
         $s.TargetPath = {}; $s.WorkingDirectory = {}; $s.Save()",
        ps_lit(&desktop.display().to_string()),
        ps_lit(&exe.display().to_string()),
        ps_lit(
            &Path::new(&options.install_dir)
                .join("app")
                .display()
                .to_string()
        ),
    ))?;
    log(app, format!("  shortcut: {}", desktop.display()));
    Ok(())
}

#[cfg(target_os = "linux")]
fn shortcut(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "no HOME".to_string())?;
    let dir = Path::new(&home).join(".local/share/applications");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {dir:?}: {e}"))?;
    let entry = dir.join("regent.desktop");
    let exe = app_exe(&options.install_dir);
    // `env VAR=… exe` rather than a bare Exec: the app resolves the deacon via
    // REGENT_DEACON_PATH, and a desktop launcher inherits none of the user's
    // shell profile. See pin_deacon.
    std::fs::write(
        &entry,
        format!(
            "[Desktop Entry]\nType=Application\nName=Regent\nComment=Built to serve\n\
             Exec=env REGENT_DEACON_PATH={} {}\nTerminal=false\nCategories=Development;\n",
            deacon_path(&options.install_dir).display(),
            exe.display()
        ),
    )
    .map_err(|e| format!("write {entry:?}: {e}"))?;
    log(app, format!("  shortcut: {}", entry.display()));
    Ok(())
}

#[cfg(target_os = "macos")]
fn shortcut(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    // macOS has no desktop-shortcut convention worth honouring; an alias to the
    // executable is the closest thing, and Spotlight already finds it.
    let _ = options;
    log(
        app,
        "  (no desktop shortcut on macOS — skipped)".to_string(),
    );
    Ok(())
}

/// The uninstaller is this same binary under another name — `main` routes on it.
/// Copying ourselves keeps one design, one progress UI, and one set of screens
/// instead of a second app to keep in sync. It costs ~10MB: the payload we were
/// launched with is a sibling resource, not part of the executable, so the copy
/// carries none of it.
#[cfg(windows)]
pub(crate) const UNINSTALLER_NAME: &str = "uninstall.exe";
#[cfg(not(windows))]
pub(crate) const UNINSTALLER_NAME: &str = "uninstall";

#[cfg(windows)]
fn place_uninstaller(app: &AppHandle, dir: &str) -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| format!("cannot locate myself: {e}"))?;
    let dest = Path::new(dir).join(UNINSTALLER_NAME);
    std::fs::copy(&me, &dest).map_err(|e| format!("copy uninstaller to {dest:?}: {e}"))?;
    log(app, format!("  uninstaller: {}", dest.display()));
    Ok(dest)
}

#[cfg(windows)]
fn uninstall_entry(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let dir = &options.install_dir;
    let exe = place_uninstaller(app, dir)?;

    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Regent";
    let uninstall = format!("\"{}\"", exe.display());
    // reg.exe is invoked directly rather than through PowerShell: the values
    // contain paths and nested quotes, and passing them as argv means there is
    // no shell to quote for.
    for (name, ty, data) in [
        ("DisplayName", "REG_SZ", "Regent"),
        ("DisplayVersion", "REG_SZ", env!("CARGO_PKG_VERSION")),
        ("Publisher", "REG_SZ", "Regent33"),
        ("InstallLocation", "REG_SZ", dir.as_str()),
        ("UninstallString", "REG_SZ", uninstall.as_str()),
        ("NoModify", "REG_DWORD", "1"),
        ("NoRepair", "REG_DWORD", "1"),
    ] {
        let out = std::process::Command::new("reg")
            .args(["add", key, "/v", name, "/t", ty, "/d", data, "/f"])
            .output()
            .map_err(|e| format!("reg add {name}: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "reg add {name}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    log(app, "  listed in Apps & features".to_string());
    Ok(())
}

/// The exact inverse of `run` — every side effect above, undone. Each step is
/// best-effort: a half-uninstalled Regent is worse than one that skips a
/// missing shortcut, so only the PATH edit (which can corrupt an env var if it
/// half-applies) is allowed to fail the stage.
#[cfg(windows)]
pub fn unwire(app: &AppHandle, dir: &Path) -> Result<(), String> {
    let bin = dir.join("bin");
    // Read-modify-write of the user PATH, straight through the registry.
    //
    // [Environment]::GetEnvironmentVariable('Path','User') expands %VAR% and
    // SetEnvironmentVariable writes REG_SZ back, so the obvious version bakes
    // every %VAR% in someone's PATH into today's value and downgrades the key
    // from REG_EXPAND_SZ — uninstalling Regent is no excuse to damage their
    // environment. Mirrors Add-UserPath in scripts/install.ps1.
    //
    // Comparison is case-insensitive and separator-normalised: what went in
    // came from a text field, and `C:\X\bin` and `c:/x/bin` are one directory.
    powershell(&format!(
        "$bin = ({bin}.TrimEnd('\\','/') -replace '/','\\')\n\
         $key = Get-Item 'HKCU:\\Environment'\n\
         $raw = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')\n\
         $kind = try {{ $key.GetValueKind('Path') }} catch {{ 'ExpandString' }}\n\
         $kept = $raw -split ';' | Where-Object {{ $_ -and \
         ($_.TrimEnd('\\','/') -replace '/','\\') -ine $bin }}\n\
         Set-ItemProperty 'HKCU:\\Environment' -Name Path -Value ($kept -join ';') -Type $kind\n\
         Remove-ItemProperty 'HKCU:\\Environment' -Name 'REGENT_DEACON_PATH' \
         -ErrorAction SilentlyContinue\n\
         if (-not ('Regent.Env' -as [type])) {{ Add-Type -Namespace Regent -Name Env \
         -MemberDefinition '[DllImport(\"user32.dll\", SetLastError=true, CharSet=CharSet.Auto)] \
         public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, \
         string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);' }}\n\
         $out = [UIntPtr]::Zero\n\
         [void][Regent.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, \
         'Environment', 2, 5000, [ref]$out)",
        bin = ps_lit(&bin.display().to_string()),
    ))?;
    log(app, "  removed PATH entry and deacon pin".into());

    if let Ok(profile) = std::env::var("USERPROFILE") {
        let lnk = Path::new(&profile).join("Desktop").join("Regent.lnk");
        if lnk.exists() {
            let _ = std::fs::remove_file(&lnk);
            log(app, format!("  removed {}", lnk.display()));
        }
    }

    let _ = std::process::Command::new("reg")
        .args([
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Regent",
            "/f",
        ])
        .output();
    log(app, "  removed the Apps & features entry".into());
    Ok(())
}

#[cfg(not(windows))]
pub fn unwire(app: &AppHandle, dir: &Path) -> Result<(), String> {
    let _ = dir;
    for p in [
        std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/bin/regent")),
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".local/share/applications/regent.desktop")),
    ]
    .into_iter()
    .flatten()
    {
        if p.exists() {
            let _ = std::fs::remove_file(&p);
            log(app, format!("  removed {}", p.display()));
        }
    }
    Ok(())
}

/// A POSIX shell single-quoted literal. '' cannot nest, so a quote is closed,
/// escaped, and reopened. Same boundary as `ps_lit`: the path is user input, and
/// `rm -rf` is the last place to discover that.
#[cfg(not(windows))]
fn sh_lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// macOS/Linux keep a script rather than the GUI uninstaller Windows gets.
/// There is no Add/Remove-Programs to register with, and copying ourselves is
/// not portable: an AppImage's `current_exe()` points inside its own mount, so
/// the copy would be a binary that cannot run on its own.
#[cfg(not(windows))]
fn uninstall_entry(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let script = Path::new(&options.install_dir).join("uninstall.sh");
    std::fs::write(
        &script,
        format!(
            "#!/usr/bin/env sh\n\
             # Removes Regent. Your ~/.regent data is left untouched.\n\
             set -eu\n\
             rm -f \"$HOME/.local/bin/regent\" \"$HOME/.local/share/applications/regent.desktop\"\n\
             rm -rf {}\n",
            sh_lit(&options.install_dir)
        ),
    )
    .map_err(|e| format!("write {script:?}: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
    }
    log(app, format!("  uninstaller: {}", script.display()));
    Ok(())
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
    use super::*;

    // The install path is typed by hand into the Location screen. These two
    // functions are the only thing standing between a name like O'Brien and a
    // `rm -rf` / registry write that means something other than intended.
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
    #[cfg(not(windows))]
    fn sh_lit_escapes_quotes() {
        assert_eq!(sh_lit("/opt/Regent"), "'/opt/Regent'");
        assert_eq!(sh_lit("/home/o'brien/Regent"), r"'/home/o'\''brien/Regent'");
        assert_eq!(sh_lit("'; rm -rf /; '"), r"''\''; rm -rf /; '\'''");
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
