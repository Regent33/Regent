//! Registering the uninstaller, and undoing every side effect `wire::run` had.
//!
//! Windows: the uninstaller is this same binary copied under another name
//! (`super::UNINSTALLER_NAME`), an Apps & features entry points at it, and
//! `unwire` reverses the PATH edit, the deacon pin, both shortcuts, and that
//! registry entry. Each undo step is best-effort — a half-uninstalled Regent is
//! worse than one that skips a missing shortcut, so only the PATH edit (which
//! can corrupt an env var if it half-applies) is allowed to fail the stage.
//!
//! macOS/Linux get an `uninstall.sh` and a file-based cleanup instead: there is
//! no Add/Remove-Programs to register with, and an AppImage's `current_exe()`
//! points inside its own mount, so a self-copy could not run on its own.

use crate::{log, InstallOptions};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

// ---- Windows: register ---------------------------------------------------

#[cfg(windows)]
fn place_uninstaller(app: &AppHandle, dir: &str) -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| format!("cannot locate myself: {e}"))?;
    let dest = Path::new(dir).join(super::UNINSTALLER_NAME);
    std::fs::copy(&me, &dest).map_err(|e| format!("copy uninstaller to {dest:?}: {e}"))?;
    log(app, format!("  uninstaller: {}", dest.display()));
    Ok(dest)
}

#[cfg(windows)]
pub(super) fn entry(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
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

// ---- Windows: undo -------------------------------------------------------

#[cfg(windows)]
pub(crate) fn unwire(app: &AppHandle, dir: &Path) -> Result<(), String> {
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
    super::powershell(&format!(
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
        bin = super::ps_lit(&bin.display().to_string()),
    ))?;
    log(app, "  removed PATH entry and deacon pin".into());

    // Both shortcuts the wire stage may have placed — Desktop (optional) and
    // Start Menu (always). Built through the same helpers as the writers so
    // the remover cannot target a different path than what was created.
    for lnk in [
        std::env::var("USERPROFILE")
            .ok()
            .map(|p| super::shortcuts::desktop_lnk(&p)),
        std::env::var("APPDATA")
            .ok()
            .map(|p| super::shortcuts::start_menu_lnk(&p)),
    ]
    .into_iter()
    .flatten()
    {
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
pub(crate) fn unwire(app: &AppHandle, dir: &Path) -> Result<(), String> {
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

// ---- non-Windows: register -----------------------------------------------

/// A POSIX shell single-quoted literal. '' cannot nest, so a quote is closed,
/// escaped, and reopened. Same boundary as `ps_lit`: the path is user input,
/// and `rm -rf` is the last place to discover that.
#[cfg(not(windows))]
fn sh_lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(not(windows))]
pub(super) fn entry(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn sh_lit_escapes_quotes() {
        assert_eq!(sh_lit("/opt/Regent"), "'/opt/Regent'");
        assert_eq!(sh_lit("/home/o'brien/Regent"), r"'/home/o'\''brien/Regent'");
        assert_eq!(sh_lit("'; rm -rf /; '"), r"''\''; rm -rf /; '\'''");
    }
}
