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

const SAMPLE_RATE: u32 = 24_000; // the rate the Realtime API speaks/expects

/// PCM16 samples → the base64 little-endian bytes the API wants.
fn pcm_to_b64(pcm: &[i16]) -> String {
    let mut bytes = Vec::with_capacity(pcm.len() * 2);
    for s in pcm {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    BASE64_STANDARD.encode(bytes)
}

/// base64 little-endian bytes → PCM16 samples (drops a trailing odd byte).
fn b64_to_pcm(b64: &str) -> Option<Vec<i16>> {
    let bytes = BASE64_STANDARD.decode(b64).ok()?;
    Some(
        bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect(),
    )
}

/// Caller audio → an `input_audio_buffer.append` client event. The transport
/// resamples to 24 kHz before this; we don't resample here.
pub fn encode_audio(frame: &AudioFrame) -> Value {
    json!({ "type": "input_audio_buffer.append", "audio": pcm_to_b64(&frame.pcm) })
}

/// A tool result → the two client events that feed it back and ask the model to
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
mod tests {
    use super::*;

    #[test]
    fn audio_round_trips_through_base64() {
        let frame = AudioFrame {
            pcm: vec![0, 1, -1, 32767, -32768],
            sample_rate: SAMPLE_RATE,
        };
        let appended = encode_audio(&frame);
        assert_eq!(appended["type"], "input_audio_buffer.append");
        // decode the same base64 back via a synthetic audio.delta event
        let delta = json!({ "type": "response.audio.delta", "delta": appended["audio"] });
        assert_eq!(decode_event(&delta), Some(ProviderEvent::Audio(frame)));
    }

    #[test]
    fn decodes_a_function_call() {
        let ev = json!({
            "type": "response.function_call_arguments.done",
            "call_id": "call_42",
            "name": "weather",
            "arguments": "{\"city\":\"Pampanga\"}",
        });
        assert_eq!(
            decode_event(&ev),
            Some(ProviderEvent::ToolCall {
                id: "call_42".into(),
                name: "weather".into(),
                args: json!({ "city": "Pampanga" }),
            })
        );
    }

    #[test]
    fn decodes_barge_in_and_ignores_unknown() {
        let started = json!({ "type": "input_audio_buffer.speech_started" });
        assert_eq!(decode_event(&started), Some(ProviderEvent::SpeechStarted));
        assert_eq!(decode_event(&json!({ "type": "session.created" })), None);
    }

    #[test]
    fn tool_result_feeds_back_then_requests_a_response() {
        let [item, create] = encode_tool_result(&ToolResult {
            id: "call_42".into(),
            output: "sunny".into(),
        });
        assert_eq!(item["type"], "conversation.item.create");
        assert_eq!(item["item"]["type"], "function_call_output");
        assert_eq!(item["item"]["call_id"], "call_42");
        assert_eq!(item["item"]["output"], "sunny");
        assert_eq!(create["type"], "response.create");
    }
}
