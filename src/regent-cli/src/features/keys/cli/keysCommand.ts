// `regent keys [list | set <NAME> <VALUE> | rm <NAME>]` — manage provider API
// keys in $REGENT_HOME/.env (search providers + platform tokens). `set` upserts
// (adds if missing, updates if present); `list` shows what's configured
// (masked). The AI-model key (REGENT_API_KEY) is managed by `regent setup` and
// is protected here. The daemon/gateway read .env at launch, so changes apply
// on the next chat / gateway start.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/daemon/locate.ts";
import { style } from "@shared/ui/style.ts";

// Keys `regent keys` knows about (shown in `list`, with friendly labels). Any
// other KEY=VALUE in .env is still editable, just without a label.
const MANAGED: ReadonlyArray<{ env: string; label: string }> = [
  {
    env: "REGENT_SEARCH_PROVIDER",
    label: "search provider (brave|tavily|serpapi|exa|google_cse|duckduckgo)",
  },
  {
    env: "REGENT_SEARCH_API_KEY",
    label: "search key (generic fallback for the selected provider)",
  },
  { env: "BRAVE_API_KEY", label: "Brave Search key" },
  { env: "TAVILY_API_KEY", label: "Tavily key" },
  { env: "SERPAPI_API_KEY", label: "SerpAPI key" },
  { env: "EXA_API_KEY", label: "Exa key" },
  { env: "GOOGLE_CSE_API_KEY", label: "Google CSE key" },
  { env: "GOOGLE_CSE_CX", label: "Google CSE engine id (cx)" },
  { env: "REGENT_TELEGRAM_TOKEN", label: "Telegram bot token" },
  { env: "REGENT_DISCORD_TOKEN", label: "Discord bot token" },
];

// Never managed here — the AI-model key is set via `regent setup`.
const PROTECTED = new Set(["REGENT_API_KEY"]);

const envPath = (home: string): string => join(home, ".env");

function readLines(home: string): string[] {
  try {
    return readFileSync(envPath(home), "utf8").split(/\r?\n/);
  } catch {
    return [];
  }
}

function writeLines(home: string, lines: string[]): void {
  mkdirSync(home, { recursive: true });
  const body = lines.join("\n").replace(/\n+$/, "");
  writeFileSync(envPath(home), `${body}\n`, { mode: 0o600 });
}

const lineIndex = (lines: string[], key: string): number =>
  lines.findIndex((l) => l.trimStart().startsWith(`${key}=`));

function valueOf(lines: string[], key: string): string | undefined {
  const i = lineIndex(lines, key);
  if (i < 0) return undefined;
  return lines[i]?.slice((lines[i]?.indexOf("=") ?? -1) + 1).trim();
}

const mask = (v: string): string => (v.length <= 4 ? "••••" : `••••${v.slice(-4)}`);

export function keysCommand(profile: string, args: string[]): number {
  const home = regentHome(profile);
  const [sub = "list", name, ...rest] = args;
  switch (sub) {
    case "list":
      return listKeys(home);
    case "set":
      return setKey(home, name, rest.join(" "));
    case "rm":
    case "remove":
    case "delete":
      return removeKey(home, name);
    default:
      printError(`unknown subcommand: keys ${sub} — use list | set | rm`);
      return 1;
  }
}

function listKeys(home: string): number {
  const lines = readLines(home);
  out(style.heading("provider keys (.env)"));
  for (const { env, label } of MANAGED) {
    const v = valueOf(lines, env);
    const status = v ? style.pass(`set ${mask(v)}`) : style.grey("not set");
    out(`  ${env.padEnd(24)} ${status}  ${style.grey(label)}`);
  }
  // Surface any other (unmanaged, non-protected) keys present so nothing hides.
  const known = new Set(MANAGED.map((m) => m.env));
  const extras = lines
    .map((l) => l.trimStart().split("=", 1)[0]?.trim() ?? "")
    .filter((k) => k && !k.startsWith("#") && !known.has(k) && !PROTECTED.has(k));
  if (extras.length > 0) out(style.grey(`  other: ${[...new Set(extras)].join(", ")}`));
  out("");
  out(style.grey("set:  regent keys set <NAME> <value>   ·   remove:  regent keys rm <NAME>"));
  out(style.grey("the AI-model key (REGENT_API_KEY) is managed by `regent setup`."));
  return 0;
}

function setKey(home: string, name: string | undefined, value: string): number {
  if (!name) {
    printError("usage: regent keys set <NAME> <value>");
    return 1;
  }
  const key = name.trim().toUpperCase();
  if (PROTECTED.has(key)) {
    printError(`${key} is the AI-model key — set it with \`regent setup\`, not here.`);
    return 1;
  }
  if (!value.trim()) {
    printError(`usage: regent keys set ${key} <value>`);
    return 1;
  }
  const lines = readLines(home);
  const i = lineIndex(lines, key);
  const existed = i >= 0;
  if (existed) lines[i] = `${key}=${value.trim()}`;
  else lines.push(`${key}=${value.trim()}`);
  writeLines(home, lines);
  out(`${style.pass("✓")} ${key} ${existed ? "updated" : "added"} (${mask(value.trim())})`);
  out(style.grey("applies on your next chat / gateway start."));
  return 0;
}

function removeKey(home: string, name: string | undefined): number {
  if (!name) {
    printError("usage: regent keys rm <NAME>");
    return 1;
  }
  const key = name.trim().toUpperCase();
  if (PROTECTED.has(key)) {
    printError(`${key} is protected and cannot be removed here.`);
    return 1;
  }
  const lines = readLines(home);
  const i = lineIndex(lines, key);
  if (i < 0) {
    out(style.grey(`${key} is not set — nothing to remove.`));
    return 0;
  }
  lines.splice(i, 1);
  writeLines(home, lines);
  out(`${style.pass("✓")} ${key} removed`);
  return 0;
}
