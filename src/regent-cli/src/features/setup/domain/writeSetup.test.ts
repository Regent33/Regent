import { describe, expect, test } from "bun:test";
import { selectSetupKey, setupConfigEntries } from "./writeSetup";

describe("setup config contract", () => {
  test("does not attach an unrelated generic cloud key to a keyless local provider", () => {
    expect(selectSetupKey("", true, "generic-cloud-key")).toBe("");
    expect(selectSetupKey("explicit-local-key", true, "generic-cloud-key")).toBe(
      "explicit-local-key",
    );
    expect(selectSetupKey("", false, "generic-cloud-key")).toBe("generic-cloud-key");
  });

  test("registers a keyless local provider and makes it the primary route", () => {
    expect(
      setupConfigEntries({
        provider: "lmstudio",
        model: "local-model",
        baseUrl: "http://localhost:1234",
        constitution: true,
        keyConfigured: false,
      }),
    ).toEqual([
      ["model.provider", "lmstudio"],
      ["model.default", "local-model"],
      ["model.base_url", "http://localhost:1234"],
      ["providers.lmstudio.kind", "lmstudio"],
      ["providers.lmstudio.base_url", "http://localhost:1234"],
      ["providers.lmstudio.api_key_env", ""],
      ["providers.lmstudio.models", ["local-model"]],
      ["agents_defaults.primary", { provider: "lmstudio", model: "local-model" }],
      ["constitution.enabled", true],
    ]);
  });

  test("preserves existing provider models and protected-local key configuration", () => {
    expect(
      setupConfigEntries({
        provider: "lmstudio",
        model: "new-model",
        baseUrl: "http://localhost:1234",
        constitution: true,
        keyConfigured: false,
        existing: { api_key_env: "LMSTUDIO_API_KEY", models: ["old-model"] },
      }),
    ).toContainEqual(["providers.lmstudio.models", ["old-model", "new-model"]]);
    expect(
      setupConfigEntries({
        provider: "lmstudio",
        model: "new-model",
        baseUrl: "http://localhost:1234",
        constitution: true,
        keyConfigured: false,
        existing: { api_key_env: "LMSTUDIO_API_KEY", models: ["old-model"] },
      }),
    ).toContainEqual(["providers.lmstudio.api_key_env", "LMSTUDIO_API_KEY"]);
  });
});
