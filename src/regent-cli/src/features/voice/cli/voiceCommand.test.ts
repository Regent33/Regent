import { expect, test } from "bun:test";
import { applySpeechConfig, defaultModels, providerKeyVar } from "./voiceCommand.ts";

test("providerKeyVar maps providers to their env keys; local needs none", () => {
  expect(providerKeyVar("groq")).toBe("GROQ_API_KEY");
  expect(providerKeyVar("openai")).toBe("OPENAI_API_KEY");
  expect(providerKeyVar("qwen")).toBe("DASHSCOPE_API_KEY");
  expect(providerKeyVar("dashscope")).toBe("DASHSCOPE_API_KEY");
  expect(providerKeyVar("local")).toBeNull();
});

test("defaultModels defaults to Qwen3 (incl. local), with per-provider overrides", () => {
  expect(defaultModels("local")).toEqual({ asr: "qwen3-asr", tts: "qwen3-tts" });
  expect(defaultModels("qwen")).toEqual({ asr: "qwen3-asr", tts: "qwen3-tts" });
  expect(defaultModels("openai")).toEqual({ asr: "whisper-1", tts: "gpt-4o-mini-tts" });
  expect(defaultModels("groq").tts).toBe(""); // groq has no TTS
});

test("applySpeechConfig merges speech settings without dropping other keys", () => {
  const doc: Record<string, unknown> = { _config_version: 1, model: { default: "x" } };
  applySpeechConfig(doc, {
    provider: "local",
    asrModel: "qwen3-asr",
    ttsModel: "qwen3-tts",
    baseUrl: "",
    enabled: true,
  });
  expect(doc.model).toEqual({ default: "x" }); // untouched
  expect(doc.speech).toEqual({
    enabled: true,
    asr: { provider: "local", model: "qwen3-asr", base_url: "" },
    tts: { provider: "local", model: "qwen3-tts", base_url: "" },
  });
});
