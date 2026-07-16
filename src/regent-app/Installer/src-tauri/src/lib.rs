//! Regent Setup — Tauri shell. Streams a staged install (or uninstall) to the
//! UI over the `install-event` channel; the work lives in `install`, `wire`,
//! and `uninstall`.
//!
//! One binary, two modes. The `wire` stage copies this executable into the
//! install directory as `uninstall.exe`, and the name it was launched under
//! picks the flow — so the uninstaller is the same design, the same progress
//! UI, and the same screens, rather than a second app to keep in sync.

mod elevate;
mod install;
mod setup;
mod uninstall;
mod wire;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// Which flow this process is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Install,
    Uninstall,
}

/// Routed on the executable's own file name, not on an argument: Apps &
/// features invokes the UninstallString with no args, and a user who
/// double-clicks `uninstall.exe` in Explorer passes none either.
fn mode() -> Mode {
    let stem = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
    mode_for(
        stem.as_deref(),
        std::env::args().any(|a| a == "--uninstall"),
    )
}

/// Split out from `mode` so the routing can be tested — getting this backwards
/// means Apps & features opens the installer.
fn mode_for(exe_stem: Option<&str>, uninstall_flag: bool) -> Mode {
    // The flag is for `tauri dev`, where the binary is always regent-installer.
    let named = exe_stem.is_some_and(|n| n.eq_ignore_ascii_case("uninstall"));
    if named || uninstall_flag {
        Mode::Uninstall
    } else {
        Mode::Install
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOptions {
    pub install_dir: String,
    pub add_to_path: bool,
    pub desktop_shortcut: bool,
}

/// One frame on the `install-event` channel. Mirrors the frontend union so the
/// Progress screen can drive its staged list + live log.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InstallEvent {
    /// `status` is one of: running | done | failed.
    Stage {
        id: String,
        status: String,
    },
    Log {
        line: String,
    },
    Done,
    Failed {
        error: String,
    },
}

const CHANNEL: &str = "install-event";

fn emit(app: &AppHandle, event: InstallEvent) {
    let _ = app.emit(CHANNEL, event);
}

pub(crate) fn log(app: &AppHandle, line: String) {
    emit(app, InstallEvent::Log { line });
}

fn stage(app: &AppHandle, id: &str, status: &str) {
    emit(
        app,
        InstallEvent::Stage {
            id: id.into(),
            status: status.into(),
        },
    );
}

/// What the frontend needs before it can render: which flow, and the directory
/// it concerns. One call rather than two so there is a single point at which
/// the UI knows what it is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Startup {
    mode: Mode,
    install_dir: String,
}

#[tauri::command]
fn startup() -> Startup {
    match mode() {
        // In uninstall mode the directory is not a choice — we are standing in it.
        Mode::Uninstall => Startup {
            mode: Mode::Uninstall,
            install_dir: uninstall::install_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        },
        Mode::Install => Startup {
            mode: Mode::Install,
            install_dir: default_install_dir(),
        },
    }
}

/// Per-user default install directory (no elevation required).
fn default_install_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(|p| format!("{p}\\Programs\\Regent"))
            .unwrap_or_else(|_| "C:\\Program Files\\Regent".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(|h| format!("{h}/.local/share/Regent"))
            .unwrap_or_else(|_| "/opt/Regent".into())
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

    std::fs::create_dir_all(path).map_err(|e| explain(&e, &dir))?;
    // Creating a directory can succeed where writing files is still refused, so
    // probe with the kind of operation the install itself performs.
    let probe = path.join(".regent-write-probe");
    let probed = std::fs::write(&probe, b"").map_err(|e| explain(&e, &dir));
    let _ = std::fs::remove_file(&probe);

    // Unwind the chain we created, deepest first. remove_dir only deletes empty
    // directories, so anything that gained content in the meantime survives.
    if let Some(stop) = preexisting {
        for dir in path.ancestors().take_while(|a| *a != stop) {
            let _ = std::fs::remove_dir(dir);
        }
    }
    probed
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
async fn start_install(app: AppHandle, options: InstallOptions) -> Result<(), String> {
    tokio::spawn(async move {
        match run_stages(&app, &options).await {
            Ok(()) => emit(&app, InstallEvent::Done),
            Err(error) => emit(&app, InstallEvent::Failed { error }),
        }
    });
    Ok(())
}

/// Each stage marks itself running → done, and a failure marks that stage
/// failed before bubbling up, so the UI always shows *where* it stopped.
async fn run_stages(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    log(
        app,
        format!(
            "target={} · add_to_path={} · desktop_shortcut={}",
            options.install_dir, options.add_to_path, options.desktop_shortcut
        ),
    );

    stage(app, "core", "running");
    install::core(app, options).await.inspect_err(|_| {
        stage(app, "core", "failed");
    })?;
    stage(app, "core", "done");

    stage(app, "app", "running");
    install::app_files(app, options).await.inspect_err(|_| {
        stage(app, "app", "failed");
    })?;
    stage(app, "app", "done");

    stage(app, "wire", "running");
    wire::run(app, options).inspect_err(|_| {
        stage(app, "wire", "failed");
    })?;
    stage(app, "wire", "done");

    // Regent is in place, so Setup's own unpacked files are dead weight from
    // here on. Outside the stages deliberately: this cannot fail an install
    // that has already succeeded. On a failure we never get here, which is
    // what we want — the payload stays put for a retry.
    setup::discard(app, &options.install_dir);

    Ok(())
}

/// Kick off the staged uninstall. Same channel, same three stages, run in
/// reverse: the app comes off before the core it depends on.
#[tauri::command]
async fn start_uninstall(app: AppHandle) -> Result<(), String> {
    tokio::spawn(async move {
        match run_uninstall_stages(&app).await {
            Ok(()) => emit(&app, InstallEvent::Done),
            Err(error) => emit(&app, InstallEvent::Failed { error }),
        }
    });
    Ok(())
}

async fn run_uninstall_stages(app: &AppHandle) -> Result<(), String> {
    let dir = uninstall::install_dir()?;
    log(app, format!("removing {}", dir.display()));
    log(app, "your ~/.regent data will be left untouched".into());

    stage(app, "app", "running");
    uninstall::stop_processes(app)
        .and_then(|()| uninstall::remove_dir(app, &dir, "app"))
        .inspect_err(|_| stage(app, "app", "failed"))?;
    stage(app, "app", "done");

    stage(app, "core", "running");
    uninstall::remove_dir(app, &dir, "bin").inspect_err(|_| stage(app, "core", "failed"))?;
    stage(app, "core", "done");

    // Last: unwire, then schedule the directory (including this .exe) to go.
    stage(app, "wire", "running");
    uninstall::unwire(app, &dir)
        .and_then(|()| uninstall::schedule_self_delete(app, &dir))
        .inspect_err(|_| stage(app, "wire", "failed"))?;
    stage(app, "wire", "done");

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

    let title = match mode() {
        Mode::Install => "Regent Setup",
        Mode::Uninstall => "Uninstall Regent",
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

    #[test]
    fn mode_routes_on_the_executable_name() {
        // What Apps & features runs: our own copy, no arguments.
        assert_eq!(mode_for(Some("uninstall"), false), Mode::Uninstall);
        // file_stem() drops ".exe", and Windows filenames are case-insensitive.
        assert_eq!(mode_for(Some("Uninstall"), false), Mode::Uninstall);
        // Anything else is a setup run — including the dev binary.
        assert_eq!(mode_for(Some("regent-installer"), false), Mode::Install);
        assert_eq!(mode_for(Some("Regent Setup"), false), Mode::Install);
        assert_eq!(mode_for(None, false), Mode::Install);
        // `tauri dev` can only reach uninstall through the flag.
        assert_eq!(mode_for(Some("regent-installer"), true), Mode::Uninstall);
    }

    #[test]
    fn uninstaller_name_matches_what_mode_routes_on() {
        // These two drifting apart is silent: wire copies to one name and the
        // router looks for another, so Apps & features opens the installer.
        let stem = std::path::Path::new(wire::UNINSTALLER_NAME)
            .file_stem()
            .and_then(|s| s.to_str());
        assert_eq!(mode_for(stem, false), Mode::Uninstall);
    }
}
