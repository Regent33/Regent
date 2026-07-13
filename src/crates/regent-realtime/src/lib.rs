//! regent-realtime — the real-time voice-call engine.
//!
//! A live call is a **relay**: caller audio in → a speech-to-speech provider →
//! audio back out, plus the provider's tool calls executed through Regent's
//! tools. The provider (OpenAI Realtime / Gemini Live) owns the hard real-time
//! speech parts (VAD, turn-taking, barge-in); the **transport** (Discord via
//! `songbird`, LiveKit, …) only moves audio frames. Both sit behind channels, so
//! the engine ([`run_call`]) is transport- and provider-agnostic and fully
//! testable offline. See `docs/proposal/realtime-calls-v1.md` + ADR-021.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

pub mod local;
pub mod openai_realtime;

// LiveKit/WebRTC transport — only when the `livekit` feature is on (native libwebrtc).
#[cfg(feature = "livekit")]
pub mod livekit_transport;

#[derive(Debug, Error)]
pub enum RealtimeError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("provider: {0}")]
    Provider(String),
}

/// A chunk of PCM audio (16-bit mono) at a sample rate. Transports resample to
/// the provider's rate (OpenAI Realtime = 24 kHz) at the edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrame {
    pub pcm: Vec<i16>,
    pub sample_rate: u32,
}

/// What the speech-to-speech provider emits.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Synthesized speech to play to the caller.
    Audio(AudioFrame),
    /// The model wants to run a tool (the Realtime API's function-calling).
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    /// The caller started talking — the provider is handling barge-in; the engine
    /// just stops expecting more outbound audio for the cancelled response.
    SpeechStarted,
}

/// A tool call's result, fed back to the provider so it can keep talking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub id: String,
    pub output: String,
}

/// Executes a tool call from inside a live call. Production wraps Regent's
/// `ToolCatalog` (same use cases the chat agent runs); tests use a stub.
#[async_trait]
pub trait CallTools: Send + Sync {
    async fn invoke(&self, name: &str, args: &Value) -> String;
}

/// The engine's channel ends to the transport (Discord/LiveKit/…). The transport
/// impl spawns its own I/O tasks that pump these.
pub struct TransportEnds {
    /// Caller audio coming in; `recv() == None` ⇒ the call ended.
    pub audio_in: mpsc::Receiver<AudioFrame>,
    /// Regent's audio going out to the caller.
    pub audio_out: mpsc::Sender<AudioFrame>,
}

/// The engine's channel ends to the speech-to-speech provider.
pub struct ProviderEnds {
    /// Caller audio pushed to the provider.
    pub audio_in: mpsc::Sender<AudioFrame>,
    /// Events from the provider (audio out, tool calls, barge-in).
    pub events: mpsc::Receiver<ProviderEvent>,
    /// Tool results returned to the provider.
    pub tool_results: mpsc::Sender<ToolResult>,
}

/// Run a live call until either side closes: relay caller audio → provider,
/// provider audio → caller, and execute the provider's tool calls via `tools`.
/// Cancellation-safe — it selects over mpsc receivers (no in-flight audio is
/// dropped when the other branch fires).
pub async fn run_call(
    mut transport: TransportEnds,
    mut provider: ProviderEnds,
    tools: Arc<dyn CallTools>,
) -> Result<(), RealtimeError> {
    loop {
        tokio::select! {
            caller = transport.audio_in.recv() => {
                let Some(frame) = caller else { break }; // transport closed → call over
                if provider.audio_in.send(frame).await.is_err() {
                    break; // provider gone
                }
            }
            event = provider.events.recv() => {
                let Some(event) = event else { break }; // provider closed → call over
                match event {
                    ProviderEvent::Audio(frame) => {
                        if transport.audio_out.send(frame).await.is_err() {
                            break; // caller gone
                        }
                    }
                    ProviderEvent::ToolCall { id, name, args } => {
                        let output = tools.invoke(&name, &args).await;
                        let _ = provider.tool_results.send(ToolResult { id, output }).await;
                    }
                    ProviderEvent::SpeechStarted => {
                        tracing::debug!("caller barge-in — provider is cancelling its response");
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
