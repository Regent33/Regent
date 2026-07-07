//! Regent Desktop — the Tauri shell. It spawns `regent-deacon` (the agent
//! backend) as a hidden child and bridges its newline-delimited JSON-RPC 2.0
//! stdio to the webview via one typed `invoke` command plus `deacon-event`
//! Tauri events. No agent logic lives in the shell — it is a thin transport
//! bridge, the same protocol regent-cli and regent-voice-server already speak.

mod commands;
mod deacon;
mod voice;

use tauri::{Manager, RunEvent};

/// Build and run the desktop app.
pub fn run() {
    tauri::Builder::default()
        // External links from chat markdown open in the system browser.
        .plugin(tauri_plugin_opener::init())
        // Native OS notification for a background turn completing (see
        // shared/infrastructure/notify.ts on the webview side).
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Spawn the deacon before the window can issue a command. `block_on`
            // is fast here: spawn only forks the child and starts the reader
            // task — there is no round-trip that would stall startup.
            let handle = app.handle().clone();
            let state = tauri::async_runtime::block_on(deacon::spawn_deacon(handle));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::deacon_request,
            voice::voice_spawn
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the Regent desktop app")
        .run(|app, event| {
            // On exit, drain the deacon (stdin EOF → 2s grace → kill) so a stuck
            // backend never leaves an orphaned process or hangs shutdown.
            if let RunEvent::Exit = event {
                if let Some(state) = app.try_state::<deacon::DeaconState>() {
                    tauri::async_runtime::block_on(deacon::shutdown(&state));
                }
            }
        });
}
