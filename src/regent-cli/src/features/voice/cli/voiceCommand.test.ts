import { expect, test } from "bun:test";
import { applySpeechConfig, defaultModels, providerKeyVar } from "./voiceCommand.ts";

test("providerKeyVar maps providers to their env keys", () => {
  expect(providerKeyVar("groq")).toBe("GROQ_API_KEY");
  expect(providerKeyVar("openai")).toBe("OPENAI_API_KEY");
  expect(providerKeyVar("qwen")).toBe("DASHSCOPE_API_KEY");
  expect(providerKeyVar("dashscope")).toBe("DASHSCOPE_API_KEY");
  expect(providerKeyVar("local")).toBeNull();
});

test("defaultModels defaults to Qwen3, with per-provider overrides", () => {
  expect(defaultModels("qwen")).toEqual({ asr: "qwen3-asr", tts: "qwen3-tts" });
  expect(defaultModels("openai")).toEqual({ asr: "whisper-1", tts: "gpt-4o-mini-tts" });
  expect(defaultModels("groq").tts).toBe(""); // groq has no TTS
});

test("applySpeechConfig merges speech settings without dropping other keys", () => {
  const doc: Record<string, unknown> = { _config_version: 1, model: { default: "x" } };
  applySpeechConfig(doc, {
    provider: "qwen",
    asrModel: "qwen3-asr",
    ttsModel: "qwen3-tts",
    enabled: true,
  });
  expect(doc.model).toEqual({ default: "x" }); // untouched
  expect(doc.speech).toEqual({
    enabled: true,
    asr: { provider: "qwen", model: "qwen3-asr" },
    tts: { provider: "qwen", model: "qwen3-tts" },
  });
});
