import { expect, test } from "bun:test";
import { defaultModels, providerKeyVar, speechConfigEntries } from "./voiceProviders.ts";

test("providerKeyVar maps providers to their env keys; local needs none", () => {
  expect(providerKeyVar("groq")).toBe("GROQ_API_KEY");
  expect(providerKeyVar("openai")).toBe("OPENAI_API_KEY");
  expect(providerKeyVar("qwen")).toBe("DASHSCOPE_API_KEY");
  expect(providerKeyVar("dashscope")).toBe("DASHSCOPE_API_KEY");
  expect(providerKeyVar("local")).toBeNull();
});

test("defaultModels defaults to Qwen3 (incl. local), with per-provider overrides", () => {
  expect(defaultModels("local")).toEqual({ asr: "qwen3-asr-1.7b", tts: "qwen3-tts-1.7b" });
  expect(defaultModels("qwen")).toEqual({ asr: "qwen3-asr-1.7b", tts: "qwen3-tts-1.7b" });
  expect(defaultModels("openai")).toEqual({ asr: "whisper-1", tts: "gpt-4o-mini-tts" });
  expect(defaultModels("groq").tts).toBe(""); // groq has no TTS
});

test("speechConfigEntries writes leaf paths only, so siblings like weights survive", () => {
  const entries = speechConfigEntries({
    provider: "local",
    asrModel: "qwen3-asr",
    ttsModel: "qwen3-tts",
    baseUrl: "",
    enabled: true,
  });
  expect(entries).toEqual([
    ["speech.enabled", true],
    ["speech.asr.provider", "local"],
    ["speech.asr.model", "qwen3-asr"],
    ["speech.asr.base_url", ""],
    ["speech.tts.provider", "local"],
    ["speech.tts.model", "qwen3-tts"],
    ["speech.tts.base_url", ""],
  ]);
  // The guard that matters: no entry addresses a whole section, which would
  // replace every key under it with this command's idea of the section.
  for (const [key] of entries) expect(key).not.toBe("speech");
  expect(entries.map(([k]) => k)).not.toContain("speech.asr");
});
