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

pub fn run(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    if options.desktop_shortcut {
        shortcut(app, options)?;
    }
    uninstall_entry(app, options)
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
    std::fs::write(
        &entry,
        format!(
            "[Desktop Entry]\nType=Application\nName=Regent\nComment=Built to serve\n\
             Exec={}\nTerminal=false\nCategories=Development;\n",
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

#[cfg(windows)]
fn uninstall_entry(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    let dir = &options.install_dir;
    let script = Path::new(dir).join("uninstall.ps1");
    std::fs::write(&script, uninstall_script(dir)).map_err(|e| format!("write {script:?}: {e}"))?;

    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Regent";
    let uninstall = format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        script.display()
    );
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

/// Removes what we placed. `~/.regent` is deliberately left alone: it holds the
/// user's config, keys and memory, and uninstalling the app is not consent to
/// delete their data.
#[cfg(windows)]
fn uninstall_script(install_dir: &str) -> String {
    let dir = ps_lit(install_dir);
    format!(
        "# Removes Regent (per-user). Your ~/.regent data is left untouched.\n\
         $ErrorActionPreference = 'SilentlyContinue'\n\
         $dir = {dir}\n\
         Get-Process regent-deacon, Regent | Stop-Process -Force\n\
         $binDir = Join-Path $dir 'bin'\n\
         $kept = [Environment]::GetEnvironmentVariable('Path','User') -split ';' | \
         Where-Object {{ $_ -and $_ -ne $binDir }}\n\
         [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')\n\
         Remove-Item -LiteralPath \"$env:USERPROFILE\\Desktop\\Regent.lnk\" -Force\n\
         # -LiteralPath throughout: an install path containing [ ] would be read\n\
         # as a wildcard and silently match nothing.\n\
         Remove-Item -Recurse -Force -LiteralPath (Join-Path $dir 'bin'), (Join-Path $dir 'app')\n\
         reg delete 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Regent' /f\n\
         # This script lives in $dir, so it cannot delete its own folder while\n\
         # running — hand that to a detached shell. The path travels by env var\n\
         # rather than string interpolation, so quotes in it cannot break out.\n\
         $env:REGENT_UNINSTALL_DIR = $dir\n\
         Start-Process powershell -WindowStyle Hidden -ArgumentList '-NoProfile','-Command',\
         'Start-Sleep 2; Remove-Item -Recurse -Force -LiteralPath $env:REGENT_UNINSTALL_DIR'\n"
    )
}

#[cfg(not(windows))]
fn uninstall_entry(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    // No Add/Remove-Programs equivalent; leave a script the user can run.
    let script = Path::new(&options.install_dir).join("uninstall.sh");
    std::fs::write(
        &script,
        format!(
            "#!/usr/bin/env sh\n\
             # Removes Regent. Your ~/.regent data is left untouched.\n\
             set -eu\n\
             rm -f \"$HOME/.local/bin/regent\" \"$HOME/.local/share/applications/regent.desktop\"\n\
             rm -rf '{}'\n",
            options.install_dir
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
