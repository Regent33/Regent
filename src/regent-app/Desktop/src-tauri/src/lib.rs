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
        // Folder picker for the coding panel. Hands back a path string only —
        // the file/git work goes through the deacon bridge, not the webview.
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Manage an unstarted bridge, then prime the deacon OFF the UI
            // thread. `block_on(spawn_deacon)` used to run here — but spawn now
            // health-probes each candidate (the multi-candidate backend-recovery
            // pass), a real round-trip that boots the deacon before returning.
            // Blocking the setup hook on it held the event loop, so the webview
            // could not even begin loading until the deacon was fully ready —
            // seconds on a cold, AV-scanned first spawn. Priming in a detached
            // task lets the window paint immediately (the BootSplash covers the
            // gap) while the deacon boots in parallel with the frontend bundle.
            // The webview's first `status.get` single-flights with this prime
            // through the state mutex, so the deacon is spawned exactly once.
            app.manage(deacon::DeaconState::unstarted());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<deacon::DeaconState>();
                if let Err(e) = state.client_or_respawn(&handle).await {
                    eprintln!("regent-desktop: deacon unavailable: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::deacon_request,
            voice::voice_spawn,
            voice::voice_restart,
            voice::call_ducking_off
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
