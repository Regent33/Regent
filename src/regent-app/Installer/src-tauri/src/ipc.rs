//! The `install-event` IPC contract: the options the frontend sends to start a
//! run, the event frames it receives back, and the emit helpers. Mirrors the
//! frontend union so the Progress screen can drive its staged list + live log.

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

pub(crate) fn emit(app: &AppHandle, event: InstallEvent) {
    let _ = app.emit(CHANNEL, event);
}

pub(crate) fn log(app: &AppHandle, line: String) {
    emit(app, InstallEvent::Log { line });
}

pub(crate) fn stage(app: &AppHandle, id: &str, status: &str) {
    emit(
        app,
        InstallEvent::Stage {
            id: id.into(),
            status: status.into(),
        },
    );
}
