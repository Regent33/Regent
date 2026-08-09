import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  activeProviderKey,
  probeLocalProviderEndpoint,
  providerKeyDiagnostics,
  readProviderInfo,
} from "./diagnostics";

const homes: string[] = [];
const vars = ["NVIDIA_API_KEY", "REGENT_API_KEY"];
afterEach(() => {
  for (const home of homes.splice(0)) rmSync(home, { recursive: true, force: true });
  for (const name of vars) delete process.env[name];
});

function homeWith(config: string, env = ""): string {
  const home = mkdtempSync(join(tmpdir(), "regent-doctor-"));
  homes.push(home);
  writeFileSync(join(home, "config.yaml"), config);
  if (env) writeFileSync(join(home, ".env"), env);
  return home;
}

describe("provider diagnostics", () => {
  test("reads the current primary provider, exact key env, and endpoint", () => {
    const home = homeWith(
      `agents_defaults:\n  primary:\n    provider: nvidia\n    model: nvidia/nemotron\nproviders:\n  nvidia:\n    kind: nvidia\n    api_key_env: NVIDIA_API_KEY\n    base_url: https://nim.example\n`,
    );
    expect(readProviderInfo(home)).toMatchObject({
      provider: "nvidia",
      model: "nvidia/nemotron",
      endpoint: "https://nim.example",
      keyCandidates: ["NVIDIA_API_KEY"],
      needsKey: true,
    });
  });

  test("treats an explicitly keyless local provider as healthy", () => {
    const home = homeWith(
      `agents_defaults:\n  primary: lmstudio/local-model\nproviders:\n  lmstudio:\n    kind: lmstudio\n    api_key_env: ''\n`,
    );
    expect(providerKeyDiagnostics(home)).toContain("not required for this local provider");
  });

  test("reports an offline keyless LM Studio endpoint as a connection failure", async () => {
    const home = homeWith(
      `agents_defaults:\n  primary:\n    provider: lmstudio\n    model: local-model\nproviders:\n  lmstudio:\n    kind: lmstudio\n    base_url: http://localhost:1234\n    api_key_env: ''\n    models: [local-model]\n`,
    );
    const info = readProviderInfo(home);
    const requested: string[] = [];
    const offlineFetch = (async (input: string | URL | Request) => {
      requested.push(String(input));
      throw new TypeError("Unable to connect");
    }) as unknown as typeof fetch;

    const probe = await probeLocalProviderEndpoint(info, { fetch: offlineFetch, timeoutMs: 25 });

    expect(requested).toEqual(["http://localhost:1234/v1/models"]);
    expect(probe).toEqual({
      ok: false,
      detail:
        "connection failed to http://localhost:1234/v1/models - start lmstudio or correct providers.lmstudio.base_url (Unable to connect)",
    });
  });

  test("reports the exact configured key and shell shadowing", () => {
    const home = homeWith(
      `agents_defaults:\n  primary: nvidia/nvidia/nemotron\nproviders:\n  nvidia:\n    kind: nvidia\n    api_key_env: NVIDIA_API_KEY\n`,
      "NVIDIA_API_KEY=saved-different-key\nREGENT_API_KEY=irrelevant-generic\n",
    );
    process.env.NVIDIA_API_KEY = "shell-active-key";
    const active = activeProviderKey(home, readProviderInfo(home));
    expect(active.source).toBe("shell env NVIDIA_API_KEY");
    expect(active.shadowed).toBe("NVIDIA_API_KEY");
  });
});
