//! openai_realtime — the OpenAI Realtime API mapping for the call engine.
//!
//! Pure codec between our engine types ([`AudioFrame`]/[`ProviderEvent`]/
//! [`ToolResult`]) and the Realtime WebSocket's JSON events. This is the brain
//! every transport (Discord/LiveKit/…) shares, and it's fully testable offline.
//! The WS *pump* (connect to `wss://api.openai.com/v1/realtime`, read/write these
//! values over tokio-tungstenite, with the API key) is a thin layer on top — it
//! only moves the JSON this module produces/parses, so it's added once a key is
//! wired. Realtime audio is **PCM16 mono @ 24 kHz**, base64 in the `audio` field.

use crate::{AudioFrame, ProviderEvent, ToolResult};
use base64::prelude::{BASE64_STANDARD, Engine as _};
use serde_json::{Value, json};

/// The rate the Realtime API speaks/expects. Transports resample at the edge.
pub const SAMPLE_RATE: u32 = 24_000;

/// The default speech-to-speech model — OpenAI's GPT Realtime 2. Overridable
/// per session via [`SessionConfig::model`]; named here so a caller that just
/// wants "the current one" does not hardcode a version.
pub const REALTIME_MODEL: &str = "gpt-realtime-2";

/// Default voice. The API rejects an unknown voice outright, so this is a
/// documented one rather than an invented name.
pub const DEFAULT_VOICE: &str = "marin";

/// What a call needs to negotiate before the first audio frame: which model
/// speaks, how it should behave, which voice, and which tools it may call.
///
/// Note the tool shape: Realtime takes FLAT function entries
/// (`{type, name, description, parameters}`), not Chat Completions' nested
/// `{type:"function", function:{…}}`. Passing the chat shape is accepted at the
/// socket and then silently yields a model that never calls a tool, so
/// [`encode_session_update`] rewrites nested entries rather than trusting the
/// caller to have picked the right one.
#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    /// Model id; empty means [`REALTIME_MODEL`].
    pub model: String,
    /// System prompt for the call.
    pub instructions: String,
    /// Voice id; empty means [`DEFAULT_VOICE`].
    pub voice: String,
    /// Tool definitions, in either the flat Realtime shape or the nested Chat
    /// Completions shape (both are normalized).
    pub tools: Vec<Value>,
    /// Let the server detect turns and handle barge-in (the default, and what
    /// [`decode_event`] expects — `SpeechStarted` only arrives with it on).
    /// `false` means the caller drives turns with explicit commits.
    pub manual_turns: bool,
}

/// Normalize one tool definition into the flat Realtime form. A Chat
/// Completions entry (`{"type":"function","function":{…}}`) is unwrapped;
/// anything already flat passes through.
fn realtime_tool(def: &Value) -> Value {
    let inner = def.get("function").unwrap_or(def);
    json!({
        "type": "function",
        "name": inner.get("name").cloned().unwrap_or(Value::Null),
        "description": inner.get("description").cloned().unwrap_or_default(),
        "parameters": inner
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
    })
}

/// The `session.update` client event that configures the call — sent once, right
/// after the socket opens and before any audio.
///
/// Emits the GA (`session.type: "realtime"`) shape, where audio settings nest
/// under `audio.input` / `audio.output`; the pre-GA beta shape put them flat on
/// the session. `decode_event` already tolerates both generations of server
/// event names, and this is the one GPT Realtime 2 expects.
#[must_use]
pub fn encode_session_update(cfg: &SessionConfig) -> Value {
    let model = if cfg.model.is_empty() {
        REALTIME_MODEL
    } else {
        &cfg.model
    };
    let voice = if cfg.voice.is_empty() {
        DEFAULT_VOICE
    } else {
        &cfg.voice
    };
    // Server VAD is what makes the provider own barge-in; without it the model
    // talks over the caller and `SpeechStarted` never fires.
    let turn_detection = if cfg.manual_turns {
        Value::Null
    } else {
        json!({ "type": "server_vad" })
    };
    json!({
        "type": "session.update",
        "session": {
            "type": "realtime",
            "model": model,
            "instructions": cfg.instructions,
            "audio": {
                "input": {
                    "format": { "type": "audio/pcm", "rate": SAMPLE_RATE },
                    "turn_detection": turn_detection,
                },
                "output": {
                    "format": { "type": "audio/pcm", "rate": SAMPLE_RATE },
                    "voice": voice,
                },
            },
            "tools": cfg.tools.iter().map(realtime_tool).collect::<Vec<_>>(),
            "tool_choice": "auto",
        }
    })
}

/// PCM16 samples to the base64 little-endian bytes the API wants.
fn pcm_to_b64(pcm: &[i16]) -> String {
    let mut bytes = Vec::with_capacity(pcm.len() * 2);
    for s in pcm {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    BASE64_STANDARD.encode(bytes)
}

/// base64 little-endian bytes to PCM16 samples (drops a trailing odd byte).
fn b64_to_pcm(b64: &str) -> Option<Vec<i16>> {
    let bytes = BASE64_STANDARD.decode(b64).ok()?;
    Some(
        bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect(),
    )
}

/// Caller audio to an `input_audio_buffer.append` client event. The transport
/// resamples to 24 kHz before this; we don't resample here.
pub fn encode_audio(frame: &AudioFrame) -> Value {
    json!({ "type": "input_audio_buffer.append", "audio": pcm_to_b64(&frame.pcm) })
}

/// A tool result to the two client events that feed it back and ask the model to
/// keep talking: a `function_call_output` item, then `response.create`.
pub fn encode_tool_result(result: &ToolResult) -> [Value; 2] {
    [
        json!({
            "type": "conversation.item.create",
            "item": {
                "type": "function_call_output",
                "call_id": result.id,
                "output": result.output,
            }
        }),
        json!({ "type": "response.create" }),
    ]
}

/// Parse one Realtime **server** event into a [`ProviderEvent`]. Returns `None`
/// for the many events we don't act on (session.created, deltas of text, etc.).
pub fn decode_event(event: &Value) -> Option<ProviderEvent> {
    match event.get("type")?.as_str()? {
        // streamed synthesized audio (field is "delta")
        "response.audio.delta" | "response.output_audio.delta" => {
            let pcm = b64_to_pcm(event.get("delta")?.as_str()?)?;
            Some(ProviderEvent::Audio(AudioFrame {
                pcm,
                sample_rate: SAMPLE_RATE,
            }))
        }
        // a completed function call: call_id, name, arguments (a JSON *string*)
        "response.function_call_arguments.done" => {
            let id = event.get("call_id")?.as_str()?.to_string();
            let name = event.get("name")?.as_str()?.to_string();
            let args = event
                .get("arguments")
                .and_then(Value::as_str)
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            Some(ProviderEvent::ToolCall { id, name, args })
        }
        // caller started talking — the API is cancelling its response (barge-in)
        "input_audio_buffer.speech_started" => Some(ProviderEvent::SpeechStarted),
        _ => None,
    }
}

#[cfg(test)]
#[path = "openai_realtime_tests.rs"]
mod tests;
