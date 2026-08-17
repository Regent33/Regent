// `regent keys [list | set <NAME> --stdin | rm <NAME>]` — manage provider API
// keys in $REGENT_HOME/.env (search providers + platform tokens). `set` upserts
// (adds if missing, updates if present); `list` shows what's configured
// (masked). The AI-model key (REGENT_API_KEY) is managed by `regent setup` and
// is protected here. The deacon/gateway read .env at launch, so changes apply
// on the next chat / gateway start.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { isSettableKey, MANAGED_KEYS, MAX_KEY_SLOTS } from "@features/keys/domain/keyCatalog.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { readDotenvLines, updateDotenv } from "@shared/infrastructure/storage/dotenvFile.ts";
import { style } from "@shared/ui/style.ts";

// Never managed here — the AI-model key is set via `regent setup`.
const PROTECTED = new Set(["REGENT_API_KEY"]);

const envPath = (home: string): string => join(home, ".env");

function readLines(home: string): string[] {
  return readDotenvLines(envPath(home));
}

// `KEY =value` is a live assignment: the deacon's loader splits on the first
// `=` and then trims the name, and `updateDotenv`'s own `envName` trims too --
// which is why `keys remove` deletes such a line. Only this lookup demanded
// that `=` touch the key, so `keys list` reported "not set" for a credential
// that was loaded and authenticating, and the Desktop page (which asks the
// deacon) said "set" for the same file. One grammar, or the two surfaces
// disagree about the same bytes.
const lineIndex = (lines: string[], key: string): number =>
  lines.findIndex((l) => {
    const rest = l.trimStart().slice(key.length);
    return l.trimStart().startsWith(key) && rest.trimStart().startsWith("=");
  });

function envValueOf(lines: string[], key: string): string | undefined {
  const i = lineIndex(lines, key);
  if (i < 0) return undefined;
  return lines[i]?.slice((lines[i]?.indexOf("=") ?? -1) + 1).trim();
}

const mask = (v: string): string => (v.length <= 4 ? "••••" : `••••${v.slice(-4)}`);

type ReadSecret = () => string;
const readStdin: ReadSecret = () => readFileSync(0, "utf8");

export function keysCommand(
  profile: string,
  args: string[],
  readSecret: ReadSecret = readStdin,
): number {
  const home = regentHome(profile);
  const [sub = "list", name, ...rest] = args;
  switch (sub) {
    case "list":
      return listKeys(home);
    case "set":
      return setKey(home, name, rest, readSecret);
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
  let heading = "";
  for (const { env, label, group } of MANAGED_KEYS) {
    if (group !== heading) {
      heading = group;
      out(`\n${style.heading(group)}`);
    }
    const v = envValueOf(lines, env);
    const status = v ? style.pass(`set ${mask(v)}`) : style.grey("not set");
    out(`  ${env.padEnd(24)} ${status}  ${style.grey(label)}`);
    for (let slot = 2; slot <= MAX_KEY_SLOTS; slot += 1) {
      const numbered = `${env}_${slot}`;
      const numberedValue = envValueOf(lines, numbered);
      if (numberedValue) {
        out(
          `  ${numbered.padEnd(24)} ${style.pass(`set ${mask(numberedValue)}`)}  ${style.grey(`${label} (${slot})`)}`,
        );
      }
    }
  }
  // Surface any other (unmanaged, non-protected) keys present so nothing hides.
  const known = new Set(MANAGED_KEYS.map((m) => m.env));
  const extras = lines
    .map((l) => l.trimStart().split("=", 1)[0]?.trim() ?? "")
    .filter((k) => k && !k.startsWith("#") && !known.has(k) && !PROTECTED.has(k));
  if (extras.length > 0) out(style.grey(`  other: ${[...new Set(extras)].join(", ")}`));
  out("");
  out(style.grey("set:  <clipboard> | regent keys set <NAME> --stdin"));
  out(style.grey("remove: regent keys rm <NAME>"));
  out(style.grey("the AI-model key (REGENT_API_KEY) is managed by `regent setup`."));
  return 0;
}

function setKey(
  home: string,
  name: string | undefined,
  args: readonly string[],
  readSecret: ReadSecret,
): number {
  if (!name) {
    printError("usage: regent keys set <NAME> --stdin");
    return 1;
  }
  const key = name.trim().toUpperCase();
  if (!/^[A-Z][A-Z0-9_]*$/.test(key)) {
    printError("key names may contain only uppercase letters, digits, and underscores");
    return 1;
  }
  if (PROTECTED.has(key)) {
    printError(`${key} is the AI-model key — set it with \`regent setup\`, not here.`);
    return 1;
  }
  if (!isSettableKey(key)) {
    printError(`${key} is not in Regent's managed key catalog`);
    return 1;
  }
  if (args.length !== 1 || !["--stdin", "-"].includes(args[0] ?? "")) {
    printError(
      `do not put secrets in command history; pipe the value to \`regent keys set ${key} --stdin\``,
    );
    return 1;
  }
  let streamed: string;
  try {
    streamed = readSecret();
  } catch (error) {
    printError(`could not read the key from stdin: ${String(error)}`);
    return 1;
  }
  // A normal pipe contributes one final line ending. Remove that transport
  // delimiter, then reject every remaining line break/NUL so one value can
  // never inject another NAME=VALUE assignment into `.env`.
  const value = streamed.replace(/\r?\n$/, "");
  if (/[\r\n\0]/.test(value)) {
    printError("provider keys must be a single line and cannot contain NUL bytes");
    return 1;
  }
  if (!value.trim()) {
    printError(`stdin did not contain a value for ${key}`);
    return 1;
  }
  let existed = false;
  try {
    existed = updateDotenv(envPath(home), { [key]: value.trim() }).has(key);
  } catch (error) {
    printError(`could not save ${key}: ${String(error)}`);
    return 1;
  }
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
  if (!/^[A-Z][A-Z0-9_]*$/.test(key)) {
    printError("key names may contain only uppercase letters, digits, and underscores");
    return 1;
  }
  if (PROTECTED.has(key)) {
    printError(`${key} is protected and cannot be removed here.`);
    return 1;
  }
  if (!isSettableKey(key)) {
    printError(`${key} is not in Regent's managed key catalog`);
    return 1;
  }
  let removed = false;
  try {
    removed = updateDotenv(envPath(home), { [key]: null }).has(key);
  } catch (error) {
    printError(`could not remove ${key}: ${String(error)}`);
    return 1;
  }
  if (!removed) {
    out(style.grey(`${key} is not set — nothing to remove.`));
    return 0;
  }
  out(`${style.pass("✓")} ${key} removed`);
  return 0;
}
