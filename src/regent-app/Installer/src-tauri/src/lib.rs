//! Regent Setup — Tauri shell. Streams a staged install to the UI over the
//! `install-event` channel and (Phase 2b) places the bundled prebuilt binaries.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOptions {
    pub install_dir: String,
    pub add_to_path: bool,
    pub all_users: bool,
    pub desktop_shortcut: bool,
}

/// One frame on the `install-event` channel. Mirrors the frontend union so the
/// Progress screen can drive its staged list + live log.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InstallEvent {
    /// `status` is one of: running | done | failed.
    Stage { id: String, status: String },
    Log { line: String },
    Done,
    Failed { error: String },
}

const CHANNEL: &str = "install-event";

fn emit(app: &AppHandle, event: InstallEvent) {
    let _ = app.emit(CHANNEL, event);
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
///
/// Phase 2b replaces the body of each stage with the real work: run the bundled
/// `install.ps1`/`install.sh` in a local/offline mode against the bundled
/// prebuilt archive (deacon + CLI), copy the desktop app into `install_dir`,
/// then write PATH / shortcuts / the Add-Remove-Programs uninstall entry.
/// Until the prebuilt binaries are bundled (Phase 0), each stage reports its
/// intent so the whole flow is exercisable end-to-end in the real window.
#[tauri::command]
async fn start_install(app: AppHandle, options: InstallOptions) -> Result<(), String> {
    tokio::spawn(async move {
        emit(
            &app,
            InstallEvent::Log {
                line: format!(
                    "target={} · add_to_path={} · all_users={} · desktop_shortcut={}",
                    options.install_dir,
                    options.add_to_path,
                    options.all_users,
                    options.desktop_shortcut
                ),
            },
        );

        let stages = [
            ("core", "regent-deacon + regent CLI"),
            ("app", "Regent desktop app"),
            ("wire", "PATH, shortcuts, uninstall entry"),
        ];
        for (id, what) in stages {
            emit(
                &app,
                InstallEvent::Stage {
                    id: id.into(),
                    status: "running".into(),
                },
            );
            emit(
                &app,
                InstallEvent::Log {
                    line: format!("[{id}] {what} (placement pending Phase 0 bundle)"),
                },
            );
            // TODO(Phase 2b): real placement from bundled resources.
            tokio::time::sleep(std::time::Duration::from_millis(650)).await;
            emit(
                &app,
                InstallEvent::Stage {
                    id: id.into(),
                    status: "done".into(),
                },
            );
        }
        emit(&app, InstallEvent::Done);
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![default_install_dir, start_install])
        .run(tauri::generate_context!())
        .expect("error while running Regent Setup");
}
