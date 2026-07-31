//! Unit tests for `openai_realtime` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

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
fn session_update_targets_realtime_2_with_server_barge_in() {
    let ev = encode_session_update(&SessionConfig {
        instructions: "Be brief.".into(),
        ..Default::default()
    });
    assert_eq!(ev["type"], "session.update");
    assert_eq!(ev["session"]["type"], "realtime");
    assert_eq!(ev["session"]["model"], "gpt-realtime-2");
    assert_eq!(ev["session"]["instructions"], "Be brief.");
    assert_eq!(ev["session"]["audio"]["output"]["voice"], "marin");
    // Both directions must be PCM at the rate the codec encodes/decodes, or
    // audio arrives as noise.
    assert_eq!(ev["session"]["audio"]["input"]["format"]["rate"], 24_000);
    assert_eq!(ev["session"]["audio"]["output"]["format"]["rate"], 24_000);
    // Server VAD on by default — the provider owns barge-in.
    assert_eq!(
        ev["session"]["audio"]["input"]["turn_detection"]["type"],
        "server_vad"
    );
}

#[test]
fn overrides_apply_and_manual_turns_disable_server_vad() {
    let ev = encode_session_update(&SessionConfig {
        model: "gpt-realtime-2-mini".into(),
        voice: "cedar".into(),
        manual_turns: true,
        ..Default::default()
    });
    assert_eq!(ev["session"]["model"], "gpt-realtime-2-mini");
    assert_eq!(ev["session"]["audio"]["output"]["voice"], "cedar");
    assert!(ev["session"]["audio"]["input"]["turn_detection"].is_null());
}

// A chat-shaped tool is accepted by the socket and then never called — the
// failure is silent, so the encoder normalizes instead of trusting input.
#[test]
fn chat_shaped_tools_are_flattened_to_the_realtime_shape() {
    let chat = json!({
        "type": "function",
        "function": {
            "name": "weather",
            "description": "look up weather",
            "parameters": { "type": "object", "properties": { "city": { "type": "string" } } },
        }
    });
    let flat = json!({
        "type": "function",
        "name": "clock",
        "description": "the time",
        "parameters": { "type": "object", "properties": {} },
    });
    let ev = encode_session_update(&SessionConfig {
        tools: vec![chat, flat],
        ..Default::default()
    });
    let tools = ev["session"]["tools"].as_array().expect("tools array");
    assert_eq!(
        tools[0]["name"], "weather",
        "nested entry was not unwrapped"
    );
    assert_eq!(tools[0]["description"], "look up weather");
    assert_eq!(
        tools[0]["parameters"]["properties"]["city"]["type"],
        "string"
    );
    assert!(tools[0].get("function").is_none(), "nesting must be gone");
    // An already-flat entry survives untouched.
    assert_eq!(tools[1]["name"], "clock");
    assert_eq!(ev["session"]["tool_choice"], "auto");
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
