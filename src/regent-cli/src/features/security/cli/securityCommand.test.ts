import { describe, expect, test } from "bun:test";
import { configuredKeyVars, envFileVars, looksLikeInlineSecret, scanSecrets } from "./securityCommand.ts";

describe("secret lint", () => {
  test("flags real inlined secrets", () => {
    expect(looksLikeInlineSecret("api_key", "sk-or-v1-abc123def")).toBe(true);
    expect(looksLikeInlineSecret("bot_token", "xoxb-1234-abcd")).toBe(true);
    expect(looksLikeInlineSecret("password", "hunter2")).toBe(true);
  });

  test("never flags env-var references (the healthy pattern)", () => {
    // `api_key_env: OPENROUTER_API_KEY` names WHERE the secret lives.
    expect(looksLikeInlineSecret("api_key_env", "OPENROUTER_API_KEY")).toBe(false);
    // A bare UPPER_SNAKE value is a reference, not a secret.
    expect(looksLikeInlineSecret("api_key", "REGENT_API_KEY")).toBe(false);
    // Non-secret keys are never flagged regardless of value.
    expect(looksLikeInlineSecret("model", "gpt-4o")).toBe(false);
  });

  test("scanSecrets walks the name-keyed provider map without false positives", () => {
    const config = {
      providers: {
        openrouter: { kind: "openai", api_key_env: "OPENROUTER_API_KEY", models: ["x"] },
        bad: { kind: "openai", api_key: "sk-live-inline-oops" },
      },
    };
    const hits: string[] = [];
    scanSecrets(config, "", hits);
    expect(hits).toEqual(["providers.bad.api_key"]);
  });
});

describe("provider key presence", () => {
  test("collects api_key_env names from the provider map", () => {
    const config = {
      providers: {
        openrouter: { api_key_env: "OPENROUTER_API_KEY" },
        nvidia: { api_key_env: "NVIDIA_API_KEY" },
        broken: { api_key_env: 42 },
      },
    };
    expect(configuredKeyVars(config)).toEqual(["OPENROUTER_API_KEY", "NVIDIA_API_KEY"]);
    expect(configuredKeyVars(undefined)).toEqual([]);
    expect(configuredKeyVars({ providers: null })).toEqual([]);
  });

  test("envFileVars reads only non-empty assignments", () => {
    const vars = envFileVars(
      "# comment\nOPENROUTER_API_KEY=sk-abc\nEMPTY=\n  SPACED = v \nNOEQ\n=bad\n",
    );
    expect(vars.has("OPENROUTER_API_KEY")).toBe(true);
    expect(vars.has("SPACED")).toBe(true);
    expect(vars.has("EMPTY")).toBe(false);
    expect(vars.has("NOEQ")).toBe(false);
  });
});
