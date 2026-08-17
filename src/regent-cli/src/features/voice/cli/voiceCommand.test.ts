import { afterEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deaconCandidates } from "@shared/infrastructure/deacon/locate.ts";
import { voiceCommand } from "./voiceCommand.ts";
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

// --- how the speech key gets in -------------------------------------------
// It comes from a pipe. `--key <secret>` is gone, and gone means refused before
// any file is touched — a token already written to the shell's history cannot
// be recalled by a warning printed afterwards.

let home = "";
const priorHome = process.env.REGENT_HOME;

function freshHome(): string {
  home = mkdtempSync(join(tmpdir(), "regent-voice-"));
  process.env.REGENT_HOME = home;
  return home;
}

const wrote = (name: string): boolean => existsSync(join(home, name));

afterEach(() => {
  if (home) rmSync(home, { recursive: true, force: true });
  home = "";
  if (priorHome === undefined) delete process.env.REGENT_HOME;
  else process.env.REGENT_HOME = priorHome;
});

describe("regent voice setup key intake", () => {
  test("--key with a value is refused before .env or config.yaml exist", async () => {
    freshHome();
    const code = await voiceCommand("", [
      ...["setup", "--provider", "groq"],
      ...["--key", "gsk-visible-in-history"],
      "--no-enable",
    ]);
    expect(code).toBe(2);
    expect(wrote(".env")).toBe(false);
    expect(wrote("config.yaml")).toBe(false);
  });

  test("--key-stdin without --provider is refused (stdin cannot answer the menu too)", async () => {
    freshHome();
    expect(await voiceCommand("", ["setup", "--key-stdin"], () => "gsk-piped\n")).toBe(2);
    expect(wrote(".env")).toBe(false);
  });

  test("an empty pipe and an injected second assignment both fail closed", async () => {
    freshHome();
    const args = ["setup", "--provider", "groq", "--key-stdin", "--no-enable"];
    expect(await voiceCommand("", args, () => "\n")).toBe(1);
    expect(await voiceCommand("", args, () => "gsk-a\nREGENT_API_KEY=stolen\n")).toBe(1);
    expect(wrote(".env")).toBe(false);
    expect(wrote("config.yaml")).toBe(false);
  });
});

// The write half needs the real config writer (the deacon binary), exactly like
// the onboarding integration tests.
describe.skipIf(deaconCandidates().length === 0)("regent voice setup --key-stdin storage", () => {
  test("the piped key lands in .env under both speech names", async () => {
    freshHome();
    const code = await voiceCommand(
      "",
      ["setup", "--provider", "groq", "--key-stdin", "--no-enable"],
      () => "gsk-piped-not-a-real-key\n",
    );
    expect(code).toBe(0);
    const saved = readFileSync(join(home, ".env"), "utf8");
    expect(saved).toContain("REGENT_SPEECH_API_KEY=gsk-piped-not-a-real-key");
    expect(saved).toContain("GROQ_API_KEY=gsk-piped-not-a-real-key");
  }, 60_000);
});
