//! Regent Setup — Tauri shell. Streams a staged install (or uninstall) to the
//! UI over the `install-event` channel; the work lives in `install`, `wire`,
//! and `uninstall`. This file owns the tauri commands + `run()`; the IPC
//! contract is in `ipc`, flow/mode routing in `flow`, the staged runners in
//! `stages`.
//!
//! One binary, two modes. On Windows the `wire` stage copies this executable
//! into the install directory as `uninstall.exe`, and the name it was launched
//! under picks the flow. On macOS/Linux no copy exists (an AppImage cannot
//! copy itself out of its mount) — instead `startup` reports any install it
//! detects, and the welcome screen offers to remove it, flipping the UI into
//! the same uninstall flow. Either way the uninstaller is the same design,
//! the same progress UI, and the same screens, rather than a second app to
//! keep in sync.

mod elevate;
mod flow;
mod install;
mod ipc;
mod setup;
mod stages;
mod uninstall;
mod wire;

// The install/wire/setup stages reach these by their historical crate-root
// paths; the definitions now live in `ipc`.
pub(crate) use ipc::{InstallOptions, log};

use tauri::AppHandle;

#[tauri::command]
fn startup() -> flow::Startup {
    match flow::mode() {
        // In uninstall mode the directory is not a choice — it is found for us.
        flow::Mode::Uninstall => flow::Startup {
            mode: flow::Mode::Uninstall,
            install_dir: uninstall::install_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            existing_install: None,
        },
        flow::Mode::Install => {
            // Prefill the location with the install we found: "Reinstall" on
            // a custom-located Regent must land on top of it, not quietly
            // start a second copy in the default directory.
            let existing = flow::existing_install();
            flow::Startup {
                mode: flow::Mode::Install,
                install_dir: existing.clone().unwrap_or_else(flow::default_install_dir),
                existing_install: existing,
            }
        }
    }
}

/// Can we actually install where the user is pointing?
///
/// The Location field is free text. Setup runs elevated when the UAC prompt is
/// accepted — but a *declined* prompt still installs per-user, and even
/// elevated there are unwritable targets (a read-only drive, a network share
/// gone away). Without this the attempt dies several stages later inside
/// install.ps1 and surfaces a raw PowerShell stack trace. Checked at the
/// boundary, while the field is still in front of the person who typed it.
///
/// Creating the directory is the check: permission is not reliably knowable on
/// Windows without attempting the write. But the check must not leave litter —
/// a declined install used to strand an empty `D:\Program Files\Regent` that
/// needed administrator rights just to delete — so whatever this creates, it
/// removes again; the install stage recreates it moments later if confirmed.
#[tauri::command]
fn check_location(dir: String) -> Result<(), String> {
    let path = std::path::Path::new(dir.trim());
    if path.is_relative() {
        return Err("Choose a full path, like C:\\Users\\you\\Regent.".into());
    }

    // Remember the part that already existed, so only OUR directories go.
    let preexisting = path
        .ancestors()
        .find(|a| a.exists())
        .map(std::path::Path::to_path_buf);

    // No `?` on the create: create_dir_all can plant SOME ancestors and then
    // fail deeper (a permission wall three levels down), and an early return
    // would skip the unwind below — stranding exactly the litter this function
    // exists to avoid. Both failure paths must fall through to the cleanup.
    let result = std::fs::create_dir_all(path)
        .map_err(|e| explain(&e, &dir))
        .and_then(|()| {
            // Creating a directory can succeed where writing files is still
            // refused, so probe with the operation the install itself performs.
            let probe = path.join(".regent-write-probe");
            let probed = std::fs::write(&probe, b"").map_err(|e| explain(&e, &dir));
            let _ = std::fs::remove_file(&probe);
            probed
        });

    // Unwind the chain we created, deepest first. remove_dir only deletes empty
    // directories, so anything that gained content in the meantime survives.
    if let Some(stop) = preexisting {
        for created in path.ancestors().take_while(|a| *a != stop) {
            let _ = std::fs::remove_dir(created);
        }
    }
    result
}

/// Turn an io::Error into something worth reading on a wizard screen. The
/// permission case is the one people actually hit, and "Access is denied
/// (os error 5)" does not tell them what to do about it.
fn explain(e: &std::io::Error, dir: &str) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        return format!(
            "{dir} needs administrator rights. Regent installs just for you, \
             so pick a folder you own — your user profile, for example."
        );
    }
    format!("Can't use {dir}: {e}")
}

/// Kick off the staged install. Returns immediately; progress arrives on the
/// `install-event` channel.
#[tauri::command]
async fn start_install(app: AppHandle, options: ipc::InstallOptions) -> Result<(), String> {
    tokio::spawn(async move {
        match stages::run_stages(&app, &options).await {
            Ok(()) => ipc::emit(&app, ipc::InstallEvent::Done),
            Err(error) => ipc::emit(&app, ipc::InstallEvent::Failed { error }),
        }
    });
    Ok(())
}

/// Kick off the staged uninstall. Same channel, same three stages, run in
/// reverse: the app comes off before the core it depends on.
#[tauri::command]
async fn start_uninstall(app: AppHandle) -> Result<(), String> {
    tokio::spawn(async move {
        match stages::run_uninstall_stages(&app).await {
            Ok(()) => ipc::emit(&app, ipc::InstallEvent::Done),
            Err(error) => ipc::emit(&app, ipc::InstallEvent::Failed { error }),
        }
    });
    Ok(())
}

/// Opens the app we just installed, then quits the installer.
#[tauri::command]
fn launch_app(app: AppHandle, install_dir: String) -> Result<(), String> {
    let exe = std::path::Path::new(&install_dir)
        .join("app")
        .join(if cfg!(windows) {
            "Regent.exe"
        } else {
            "Regent"
        });

    // On Windows the installer runs elevated (elevate.rs), and a direct child
    // would inherit the admin token — an app that browses, downloads, and runs
    // a deacon has no business starting life as administrator, and UIPI blocks
    // drag-and-drop into elevated windows besides. Explorer launches it with
    // the normal desktop token instead. The deacon pin still arrives:
    // pin_deacon's SetEnvironmentVariable broadcast WM_SETTINGCHANGE, which
    // Explorer honours, so its children see the fresh user environment.
    //
    // If Explorer itself cannot be spawned, fall back to a direct (elevated)
    // launch with the pin passed explicitly — a child inherits OUR stale
    // pre-pin environment. An elevated first run beats a dead Launch button.
    #[cfg(windows)]
    let spawned = std::process::Command::new("explorer.exe")
        .arg(&exe)
        .spawn()
        .map(|_| ())
        .or_else(|_| {
            std::process::Command::new(&exe)
                .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
                .env("REGENT_DEACON_PATH", wire::deacon_path(&install_dir))
                .spawn()
                .map(|_| ())
        });
    #[cfg(not(windows))]
    let spawned = std::process::Command::new(&exe)
        .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
        .env("REGENT_DEACON_PATH", wire::deacon_path(&install_dir))
        .spawn()
        .map(|_| ());

    spawned.map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    app.exit(0);
    Ok(())
}

/// Closes the uninstaller once the detached cleanup has been scheduled — it
/// cannot delete our directory while we still hold this .exe open.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before any window: an elevated copy takes over from here, and this one
    // has nothing left to show.
    if !elevate::ensure_elevated() {
        return;
    }

    let title = match flow::mode() {
        flow::Mode::Install => "Regent Setup",
        flow::Mode::Uninstall => "Uninstall Regent",
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            startup,
            check_location,
            start_install,
            start_uninstall,
            launch_app,
            quit
        ])
        .setup(move |app| {
            // The window is declared in tauri.conf with the installer's title;
            // the OS title bar is the only chrome either mode has, so it has to
            // say which one you are looking at.
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(title);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Regent Setup");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_location_leaves_no_litter() {
        // The bug this pins: probing D:\Program Files\Regent then backing out
        // stranded an empty directory that needed administrator to delete.
        let base = std::env::temp_dir().join(format!("regent-loc-{}", std::process::id()));
        let target = base.join("a").join("b");
        assert!(check_location(target.display().to_string()).is_ok());
        assert!(
            !base.exists(),
            "probe left {base:?} behind — the created chain must be unwound"
        );

        // A directory that already existed is not ours to remove.
        std::fs::create_dir_all(&base).unwrap();
        assert!(check_location(base.display().to_string()).is_ok());
        assert!(base.exists(), "pre-existing target must survive the probe");
        let _ = std::fs::remove_dir(&base);
    }
}
