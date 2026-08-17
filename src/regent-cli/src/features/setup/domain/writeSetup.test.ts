import { describe, expect, test } from "bun:test";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { selectSetupKey, setupConfigEntries, writeEnv } from "./writeSetup";

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

test("an ACL failure leaves the previous secret file untouched", () => {
  const home = mkdtempSync(join(tmpdir(), "regent-setup-acl-"));
  try {
    writeFileSync(join(home, ".env"), "REGENT_API_KEY=old-secret\nOTHER=value\n");
    expect(() =>
      writeEnv(home, "new-secret", () => {
        throw new Error("ACL denied");
      }),
    ).toThrow("ACL denied");
    expect(readFileSync(join(home, ".env"), "utf8")).toBe(
      "REGENT_API_KEY=old-secret\nOTHER=value\n",
    );
    expect(
      readdirSync(home).filter((name) => name.includes(".tmp.") || name.endsWith(".lock")),
    ).toEqual([]);
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

test("setup normalizes duplicate model-key assignments under the shared lock", () => {
  const home = mkdtempSync(join(tmpdir(), "regent-setup-duplicates-"));
  try {
    writeFileSync(
      join(home, ".env"),
      "REGENT_API_KEY=old-one\nOTHER=value\n REGENT_API_KEY=old-two\n",
    );
    writeEnv(home, "canonical");
    const saved = readFileSync(join(home, ".env"), "utf8");
    expect(saved.match(/REGENT_API_KEY=/g)?.length).toBe(1);
    expect(saved).toContain("REGENT_API_KEY=canonical");
    expect(saved).toContain("OTHER=value");
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});

test("setup cannot inject another environment assignment through its key", () => {
  const home = mkdtempSync(join(tmpdir(), "regent-setup-injection-"));
  try {
    expect(() => writeEnv(home, "safe\nREGENT_AUTO_APPROVE=1")).toThrow("single line");
    expect(() => writeEnv(home, "safe\0value")).toThrow("NUL");
    expect(readdirSync(home)).toEqual([]);
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
});
