//! regent-voice-server — the local speech server binary. Serves the
//! OpenAI-compatible ASR/TTS endpoints and the hands-free browser call at
//! `/call`, loopback-only (see `infra::http` for the security posture).

use regent_voice_server::infra::engines::Engines;
use regent_voice_server::infra::http::{AppState, router};
use regent_voice_server::infra::spawn::{AgentStatus, spawn_agent};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("REGENT_VOICE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000);
    let state = Arc::new(AppState {
        engines: RwLock::new(Engines::unavailable(
            "local engines still loading (first run also downloads models)",
        )),
        deacon: RwLock::new(None),
        agent_note: RwLock::new("warming up".into()),
        token: format!("{:032x}", rand::random::<u128>()),
    });

    println!("regent-voice-server → http://localhost:{port}");
    println!("  voice call: http://localhost:{port}/call");

    // Engines load (and on first run download) in the background — the server
    // is reachable immediately; /health carries live progress (download MBs)
    // and flips `warm` when they're in.
    let engine_state = Arc::clone(&state);
    tokio::spawn(async move {
        let note_state = Arc::clone(&engine_state);
        let engines = tokio::task::spawn_blocking(move || {
            Engines::from_env_with(&move |msg: String| {
                println!("  {msg}");
                note_state.engines.blocking_write().note = msg;
            })
        })
        .await
        .unwrap_or_else(|e| Engines::unavailable(&format!("engine load panicked: {e}")));
        if engines.ready() {
            println!("  ✓ local engines ready (whisper + kokoro, ONNX)");
        } else {
            println!("  ⚠ {}", engines.note);
        }
        *engine_state.engines.write().await = engines;
    });

    // Warm the agent deacon in the background so the server starts instantly
    // and the FIRST call is already agentic (tools/memory).
    let warm_state = Arc::clone(&state);
    tokio::spawn(async move {
        match spawn_agent().await {
            AgentStatus::Ready(rpc) => {
                *warm_state.deacon.write().await = Some(rpc);
                *warm_state.agent_note.write().await = "ready".into();
                println!("  ✓ agent brain ready — voice runs the full agent (tools/memory)");
            }
            AgentStatus::Unavailable(reason) => {
                *warm_state.agent_note.write().await = reason.clone();
                println!("  ⚠ agent voice off ({reason}) → calls echo until it's available");
            }
        }
    });

    // Loopback only — never world-exposed; pair with the Host check inside.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bind 127.0.0.1");
    axum::serve(listener, router(state)).await.expect("serve");
}
