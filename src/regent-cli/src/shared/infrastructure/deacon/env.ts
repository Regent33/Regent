// The child-process environment contract for the spawned deacon. Merges
// $REGENT_HOME/.env for secrets (the real environment always wins, so .env
// never overrides an explicit export) — mirrors the Go appendDotEnv.
import { readFileSync } from "node:fs";
import { join } from "node:path";

/** Build the deacon's env: real env + forced REGENT_HOME + merged .env + clock. */
export function buildEnv(home: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, REGENT_HOME: home };
  for (const [key, value] of readDotEnv(home)) {
    if (env[key] === undefined) env[key] = value;
  }
  // The deacon has no clock dep; hand it the wall-clock so the agent can answer
  // date/time. Set at spawn (a fresh deacon per `regent` invocation = current).
  env.REGENT_NOW = new Date().toLocaleString();
  // Desktop control available by default in the CLI, matching the voice call's
  // default — safe here because every mutating action still asks in the TUI
  // (interactive approval). REGENT_COMPUTER_USE=0 in env/.env disables it.
  if (env.REGENT_COMPUTER_USE === undefined) env.REGENT_COMPUTER_USE = "1";
  return env;
}

/** Parse $REGENT_HOME/.env into key/value pairs, ignoring blanks and comments. */
function readDotEnv(home: string): Array<[string, string]> {
  let data: string;
  try {
    data = readFileSync(join(home, ".env"), "utf8");
  } catch {
    return [];
  }
  const out: Array<[string, string]> = [];
  for (const raw of data.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq <= 0) continue;
    const key = line.slice(0, eq).trim();
    const value = line
      .slice(eq + 1)
      .trim()
      .replace(/^"|"$/g, "");
    out.push([key, value]);
  }
  return out;
}
