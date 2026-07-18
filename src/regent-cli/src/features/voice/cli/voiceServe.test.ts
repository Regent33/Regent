import { describe, expect, test } from "bun:test";
import { CALL_PROTOCOL, classifySpeechHealth, voiceModelSelection } from "./voiceServe.ts";

describe("speech backend protocol", () => {
  test("old Rust servers are stale while current Rust and Python stay usable", () => {
    expect(classifySpeechHealth(null)).toBe("down");
    expect(classifySpeechHealth({ engine: "regent-voice-server (rust)" })).toBe("stale");
    expect(
      classifySpeechHealth({
        engine: "regent-voice-server (rust)",
        call_protocol: CALL_PROTOCOL - 1,
      }),
    ).toBe("stale");
    expect(
      classifySpeechHealth({ engine: "regent-voice-server (rust)", call_protocol: CALL_PROTOCOL }),
    ).toBe("current");
    expect(classifySpeechHealth({ engine: "faster-whisper+kokoro" })).toBe("current");
  });
});

describe("Butler model routing", () => {
  test("follows chat's provider/model pair and never carries the legacy base URL across providers", () => {
    expect(
      voiceModelSelection({
        model: { default: "legacy-model", base_url: "https://legacy.example/v1" },
        agents_defaults: { primary: { provider: "openrouter", model: "openai/gpt-next" } },
        speech: { call: { fast_model: "" } },
      }),
    ).toEqual({ model: "openrouter/openai/gpt-next" });
  });

  test("an explicit voice-only fast model remains the intentional override", () => {
    expect(
      voiceModelSelection({
        agents_defaults: { primary: { provider: "nvidia", model: "nvidia/main" } },
        speech: { call: { fast_model: "ollama/qwen-local" } },
      }),
    ).toEqual({ model: "ollama/qwen-local" });
  });
});
