// `regent providers list|add|remove|test` — manage the multi-provider map
// (ADR-026). `list`/`test` query the daemon (providers.* RPC); `add`/`remove`
// edit $REGENT_HOME/config.yaml's `providers` map directly (filesystem — the
// daemon honors it at registry-build time on the next run), mirroring `tools`.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";
import { renderTable } from "@shared/ui/table.ts";
import YAML from "yaml";

// Valid wire protocols — must match the daemon's ProviderKind enum (config.rs).
const KINDS = ["anthropic", "openai", "openrouter", "groq", "deepseek", "together", "ollama"];

interface ProviderRow {
  name: string;
  kind: string;
  base_url: string | null;
  api_key_env: string;
  key_present: boolean;
  models: string[];
}

// ── Read surface (needs the daemon): list · test ────────────────────────────
export async function providersCommand(client: IRpcClient, args: string[]): Promise<number> {
  const [sub = "list", ...rest] = args;
  switch (sub) {
    case "list":
      return list(client);
    case "test":
      return test(client, rest[0]);
    default:
      printError(`unknown providers subcommand: ${sub}`);
      out(
        "usage: providers [list | add <name> --kind <k> --key-env <ENV> --models a,b [--base-url url] | remove <name> | test <name|provider/model>]",
      );
      return 1;
  }
}

async function list(client: IRpcClient): Promise<number> {
  const res = await client.call<ProviderRow[]>("providers.list", {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.length === 0) {
    out(
      style.grey(
        "no providers configured — providers add <name> --kind <k> --key-env <ENV> --models a,b",
      ),
    );
    return 0;
  }
  out(style.heading(`Providers — ${res.value.length}`));
  for (const line of renderTable(res.value, [
    { header: "NAME", get: (p) => p.name, paint: (c) => style.teal(c) },
    { header: "KIND", get: (p) => p.kind, paint: (c) => style.grey(c) },
    {
      header: "KEY",
      get: (p) => (p.key_present ? "✓" : `✗ ${p.api_key_env}`),
      paint: (c) => (c === "✓" ? style.pass(c) : style.warn(c)),
    },
    { header: "MODELS", get: (p) => p.models.join(", "), flex: true },
  ])) {
    out(line);
  }
  return 0;
}

async function test(client: IRpcClient, name: string | undefined): Promise<number> {
  if (!name) {
    printError("usage: providers test <name|provider/model>");
    return 1;
  }
  out(style.grey(`pinging ${name}…`));
  const res = await client.call<{ ok: boolean; model: string; error?: string }>(
    "providers.test",
    { name },
    30_000,
  );
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  if (res.value.ok) {
    out(`${style.pass("✓")} ${style.value(res.value.model)} responded`);
    return 0;
  }
  printError(`${res.value.model}: ${res.value.error ?? "no response"}`);
  return 1;
}

// ── Write surface (edits config.yaml): add · remove ─────────────────────────
const FLAGS = {
  kind: { type: "string", alias: "k" },
  "base-url": { type: "string" },
  "key-env": { type: "string" },
  models: { type: "string", alias: "m" },
} as const;

export function providersEditCommand(profile: string, args: string[]): number {
  const [sub = "", ...rest] = args;
  const { values, positionals } = parseFlags(rest, FLAGS);
  const name = positionals[0];
  if (!name) {
    printError(`usage: providers ${sub} <name> [flags]`);
    return 1;
  }

  const doc = loadConfig(profile);
  const providers =
    typeof doc.providers === "object" && doc.providers !== null
      ? (doc.providers as Record<string, unknown>)
      : {};

  if (sub === "add") {
    const kind = str(values.kind);
    const keyEnv = str(values["key-env"]);
    const models = str(values.models)
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    if (!KINDS.includes(kind)) {
      printError(`--kind must be one of: ${KINDS.join(", ")}`);
      return 1;
    }
    if (!keyEnv) {
      printError("--key-env <ENV_VAR_NAME> is required (the env var holding the API key)");
      return 1;
    }
    if (models.length === 0) {
      printError("--models a,b,c is required (at least one model id)");
      return 1;
    }
    const entry: Record<string, unknown> = { kind, api_key_env: keyEnv, models };
    const baseUrl = str(values["base-url"]);
    if (baseUrl) entry.base_url = baseUrl;
    providers[name] = entry;
    doc.providers = providers;
    writeConfig(profile, doc);
    out(
      `${style.pass("✓")} added provider ${style.teal(name)} (${kind}, ${models.length} model(s))`,
    );
    out(style.grey("(applies on the next `regent` command — the daemon reloads config each run)"));
    return 0;
  }

  if (sub === "remove" || sub === "rm") {
    if (!(name in providers)) {
      out(style.grey(`no provider '${name}'`));
      return 0;
    }
    delete providers[name];
    doc.providers = providers;
    writeConfig(profile, doc);
    out(`${style.pass("✓")} removed provider ${style.teal(name)}`);
    out(style.grey("(applies on the next `regent` command)"));
    return 0;
  }

  printError(`unknown providers subcommand: ${sub}`);
  return 1;
}

function loadConfig(profile: string): Record<string, unknown> {
  const path = join(regentHome(profile), "config.yaml");
  let doc: Record<string, unknown> = {};
  try {
    const parsed = YAML.parse(readFileSync(path, "utf8")) as unknown;
    if (parsed && typeof parsed === "object") doc = parsed as Record<string, unknown>;
  } catch {
    // no / invalid config.yaml — start fresh
  }
  if (doc._config_version === undefined) doc._config_version = 1;
  return doc;
}

function writeConfig(profile: string, doc: Record<string, unknown>): void {
  const home = regentHome(profile);
  mkdirSync(home, { recursive: true });
  const path = join(home, "config.yaml");
  const tmp = join(home, `config.yaml.tmp.${process.pid}`);
  writeFileSync(tmp, YAML.stringify(doc));
  renameSync(tmp, path);
}

const str = (v: string | boolean | undefined): string => (typeof v === "string" ? v : "");
