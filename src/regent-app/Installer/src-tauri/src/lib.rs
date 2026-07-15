//! Regent Setup — Tauri shell. Streams a staged install to the UI over the
//! `install-event` channel; the work itself lives in `install` and `wire`.

mod install;
mod wire;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

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

/// Per-user default install directory (no elevation required).
#[tauri::command]
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
    std::process::Command::new(&exe)
        .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    app.exit(0);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            default_install_dir,
            start_install,
            launch_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running Regent Setup");
}
