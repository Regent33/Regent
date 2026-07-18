import { describe, expect, test } from "bun:test";
import { CALL_PROTOCOL, classifySpeechHealth } from "./voiceServe.ts";

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
