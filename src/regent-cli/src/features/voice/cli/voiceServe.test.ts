import { describe, expect, test } from "bun:test";
import { classifySpeechHealth } from "./voiceServe.ts";

describe("speech backend protocol", () => {
  test("old Rust servers are stale while current Rust and Python stay usable", () => {
    expect(classifySpeechHealth(null)).toBe("down");
    expect(classifySpeechHealth({ engine: "regent-voice-server (rust)" })).toBe("stale");
    expect(classifySpeechHealth({ engine: "regent-voice-server (rust)", call_protocol: 2 })).toBe(
      "current",
    );
    expect(classifySpeechHealth({ engine: "faster-whisper+kokoro" })).toBe("current");
  });
});
