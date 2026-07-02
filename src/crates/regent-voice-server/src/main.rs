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
    let engines = Engines::from_env();
    let state = Arc::new(AppState {
        engines: engines.clone(),
        deacon: RwLock::new(None),
        agent_note: RwLock::new("warming up".into()),
        token: format!("{:032x}", rand::random::<u128>()),
    });

    println!("regent-voice-server → http://localhost:{port}");
    println!("  voice call: http://localhost:{port}/call");
    if !engines.ready() {
        println!("  ⚠ {}", engines.note);
    }

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
