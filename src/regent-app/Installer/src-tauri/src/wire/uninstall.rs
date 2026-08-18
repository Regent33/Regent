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

/// Expand `%NAME%` references the way the registry would when it hands a
/// REG_EXPAND_SZ value out. An unknown name is left as written, so it cannot
/// silently collapse to something that happens to exist.
#[cfg(windows)]
fn expand_env(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find('%') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('%') {
            Some(close) => {
                let name = &after[..close];
                match std::env::var(name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push('%');
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether the persisted `REGENT_DEACON_PATH` pin belongs to the install being
/// removed (or has gone stale) and may be dropped.
///
/// This used to be an unconditional `Remove-ItemProperty`: uninstalling one
/// Regent deleted the pin even when it named a DIFFERENT install's deacon, and
/// that install then fell back to PATH discovery. `scripts/uninstall.ps1` had
/// already worked this out (`Test-RegentPinRemovable`) — the GUI path simply
/// never mirrored it. Same three rules, same order:
///   - a pin inside the bin dir being deleted    -> remove (this install)
///   - a pin whose target no longer exists       -> remove (stale)
///   - a pin resolving to another install's exe  -> keep
#[cfg(windows)]
fn pin_is_removable(pin: &str, bin: &Path, exists: &dyn Fn(&Path) -> bool) -> bool {
    let trimmed = pin.trim().trim_matches('"');
    if trimmed.is_empty() {
        return false;
    }
    let expanded = expand_env(trimmed).replace('/', "\\");
    let pin_path = PathBuf::from(expanded.trim_end_matches('\\'));
    let bin_prefix = format!(
        "{}\\",
        bin.display()
            .to_string()
            .replace('/', "\\")
            .trim_end_matches('\\')
    );
    if pin_path
        .display()
        .to_string()
        .to_lowercase()
        .starts_with(&bin_prefix.to_lowercase())
    {
        return true;
    }
    !exists(&pin_path)
}

/// The value out of `reg query` output:
/// `    REGENT_DEACON_PATH    REG_EXPAND_SZ    C:\Program Files\...\x.exe`
///
/// Split on the TYPE token, never on whitespace: the value is a path and paths
/// have spaces in them. Taking the third whitespace-delimited field returned
/// `C:\Users\Ada` for `C:\Users\Ada Lovelace\...`, which is not under the bin
/// dir and does not exist — so the caller would have judged another install's
/// pin stale and deleted it, which is the very bug this file is fixing.
#[cfg(windows)]
fn parse_pin_line(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains("REGENT_DEACON_PATH"))?;
    let (_, after) = ["REG_EXPAND_SZ", "REG_SZ"]
        .iter()
        .find_map(|ty| line.split_once(ty))?;
    let value = after.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// The raw (unexpanded) `REGENT_DEACON_PATH` from HKCU, or `None` when unset.
#[cfg(windows)]
fn read_pin() -> Option<String> {
    let out = std::process::Command::new("reg")
        .args(["query", r"HKCU\Environment", "/v", "REGENT_DEACON_PATH"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_pin_line(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(windows)]
pub(crate) fn unwire(app: &AppHandle, dir: &Path) -> Result<(), String> {
    let bin = dir.join("bin");
    // Decided here, BEFORE the PATH edit below, so its WM_SETTINGCHANGE
    // broadcast covers this change too.
    match read_pin() {
        Some(pin) if pin_is_removable(&pin, &bin, &|p: &Path| p.is_file()) => {
            let _ = std::process::Command::new("reg")
                .args([
                    "delete",
                    r"HKCU\Environment",
                    "/v",
                    "REGENT_DEACON_PATH",
                    "/f",
                ])
                .output();
            log(
                app,
                "  removed the deacon pin (this install, or its target is gone)".into(),
            );
        }
        Some(_) => log(
            app,
            "  kept the deacon pin - it points at a different Regent install".into(),
        ),
        None => {}
    }
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
         if (-not ('Regent.Env' -as [type])) {{ Add-Type -Namespace Regent -Name Env \
         -MemberDefinition '[DllImport(\"user32.dll\", SetLastError=true, CharSet=CharSet.Auto)] \
         public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, \
         string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);' }}\n\
         $out = [UIntPtr]::Zero\n\
         [void][Regent.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, \
         'Environment', 2, 5000, [ref]$out)",
        bin = super::ps_lit(&bin.display().to_string()),
    ))?;
    log(app, "  removed the PATH entry".into());

    // Both shortcuts the wire stage may have placed — Desktop (optional) and
    // Start Menu (always). Built through the same helpers as the writers so
    // the remover cannot target a different path than what was created.
    for lnk in [
        super::shortcuts::desktop_dir().map(|d| super::shortcuts::desktop_lnk(&d)),
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

    // The pin decision the GUI uninstaller never had. Cases mirror
    // scripts/tests/verify-install.ps1 one for one, so the two uninstall paths
    // are provably answering the same question.
    #[test]
    #[cfg(windows)]
    fn pin_decision_matches_the_shell_uninstaller() {
        let bin = Path::new(r"C:\Program Files\Regent\bin");
        let always = |_: &Path| true;
        let never = |_: &Path| false;

        // Inside the bin dir being deleted: ours, remove it.
        assert!(pin_is_removable(
            r"C:\Program Files\Regent\bin\regent-deacon.exe",
            bin,
            &always
        ));
        // Same directory, written with forward slashes or a stray quote.
        assert!(pin_is_removable(
            "C:/Program Files/Regent/bin/regent-deacon.exe",
            bin,
            &always
        ));
        assert!(pin_is_removable(
            "\"C:\\Program Files\\Regent\\bin\\regent-deacon.exe\"",
            bin,
            &always
        ));
        // Case differences are the same path on Windows.
        assert!(pin_is_removable(
            r"c:\program files\regent\BIN\regent-deacon.exe",
            bin,
            &always
        ));
        // Another install, and its target really is there: keep it.
        assert!(!pin_is_removable(
            r"D:\Other\Regent\bin\regent-deacon.exe",
            bin,
            &always
        ));
        // Another install whose target has gone: stale, remove it.
        assert!(pin_is_removable(
            r"D:\Other\Regent\bin\regent-deacon.exe",
            bin,
            &never
        ));
        // Nothing pinned.
        assert!(!pin_is_removable("", bin, &always));
        assert!(!pin_is_removable("   ", bin, &always));
    }

    // A path with a space in it is the NORMAL case on Windows
    // (`C:\Users\Ada Lovelace\...`), and getting it wrong here is not a cosmetic
    // parse bug: a truncated path fails the "does the target exist" test, so the
    // caller deletes a pin that belongs to somebody else's install.
    #[test]
    #[cfg(windows)]
    fn a_pinned_path_containing_spaces_survives_parsing() {
        let out = "\r\nHKEY_CURRENT_USER\\Environment\r\n    REGENT_DEACON_PATH    \
                   REG_EXPAND_SZ    C:\\Users\\Ada Lovelace\\AppData\\Local\\Programs\\Regent\\bin\\regent-deacon.exe\r\n";
        assert_eq!(
            parse_pin_line(out).as_deref(),
            Some(r"C:\Users\Ada Lovelace\AppData\Local\Programs\Regent\bin\regent-deacon.exe")
        );
        // REG_SZ is just as valid a type for the pin.
        assert_eq!(
            parse_pin_line("    REGENT_DEACON_PATH    REG_SZ    C:\\R\\bin\\d.exe").as_deref(),
            Some(r"C:\R\bin\d.exe")
        );
        // Unset, or a value-less line: nothing to remove.
        assert_eq!(parse_pin_line("ERROR: The system was unable to find"), None);
        assert_eq!(
            parse_pin_line("    REGENT_DEACON_PATH    REG_EXPAND_SZ    "),
            None
        );
    }

    #[test]
    #[cfg(windows)]
    fn env_references_expand_and_unknown_names_survive() {
        let root = std::env::var("SystemRoot").expect("SystemRoot is always set on Windows");
        assert_eq!(expand_env("%SystemRoot%\\x"), format!("{root}\\x"));
        // An unknown name must not collapse to nothing — that could turn a
        // pin for another install into a path that looks like ours.
        assert_eq!(
            expand_env("%REGENT_NOT_A_REAL_VAR%\\x"),
            "%REGENT_NOT_A_REAL_VAR%\\x"
        );
        assert_eq!(expand_env("no percent signs"), "no percent signs");
        assert_eq!(expand_env("dangling % sign"), "dangling % sign");
    }

    #[test]
    #[cfg(not(windows))]
    fn sh_lit_escapes_quotes() {
        assert_eq!(sh_lit("/opt/Regent"), "'/opt/Regent'");
        assert_eq!(sh_lit("/home/o'brien/Regent"), r"'/home/o'\''brien/Regent'");
        assert_eq!(sh_lit("'; rm -rf /; '"), r"''\''; rm -rf /; '\'''");
    }
}
