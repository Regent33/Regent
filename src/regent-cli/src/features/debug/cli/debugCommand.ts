// `regent debug` — assemble a redacted bug-report bundle under
// $REGENT_HOME/debug/: system info, a secret-stripped copy of config.yaml, and
// the latest daemon logs. Secrets (.env) and conversation content (state.db)
// are deliberately excluded. Pure CLI (host filesystem), no daemon round-trip.
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { BRAND } from "@app/config/brand.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";

const SECRET_KEY = /(key|token|secret|password|passwd|auth|credential)/i;

// Mask the value of any `key: value` line whose key looks secret-bearing.
function redactYaml(text: string): string {
  return text
    .split("\n")
    .map((line) => {
      const m = line.match(/^(\s*[\w.-]+\s*:\s*)(.+)$/);
      if (m && m[1] && SECRET_KEY.test(m[1])) return `${m[1]}***redacted***`;
      return line;
    })
    .join("\n");
}

export function debugCommand(profile: string): number {
  const home = regentHome(profile);
  if (!existsSync(home)) {
    printError(`no profile home at ${home} — run \`regent setup\` first`);
    return 1;
  }
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const bundle = join(home, "debug", `regent-debug-${stamp}`);
  mkdirSync(bundle, { recursive: true });
  const included: string[] = [];

  const sys = [
    `regent ${BRAND.version}`,
    `generated   ${new Date().toISOString()}`,
    `platform    ${process.platform} ${process.arch}`,
    `runtime     bun ${process.versions.bun ?? "—"} · node ${process.versions.node ?? "—"}`,
    `REGENT_HOME ${home}`,
    `cwd         ${process.cwd()}`,
  ].join("\n");
  writeFileSync(join(bundle, "system.txt"), `${sys}\n`);
  included.push("system.txt");

  const cfg = join(home, "config.yaml");
  if (existsSync(cfg)) {
    writeFileSync(join(bundle, "config.redacted.yaml"), redactYaml(readFileSync(cfg, "utf8")));
    included.push("config.redacted.yaml");
  }

  const logsDir = join(home, "logs");
  if (existsSync(logsDir)) {
    const logs = readdirSync(logsDir)
      .filter((f) => f.startsWith("regent.log"))
      .sort()
      .slice(-3);
    for (const f of logs) {
      copyFileSync(join(logsDir, f), join(bundle, f));
      included.push(f);
    }
  }

  writeFileSync(
    join(bundle, "README.txt"),
    `${[
      "Regent debug bundle — safe to attach to a bug report.",
      "",
      "Included:",
      ...included.map((f) => `  - ${f}`),
      "",
      "Deliberately excluded (may contain secrets or private content):",
      "  - .env       (provider API keys)",
      "  - state.db   (conversation history, memory graph)",
    ].join("\n")}\n`,
  );
  included.push("README.txt");

  out(style.pass("✓ debug bundle written"));
  out(`  ${bundle}`);
  out(`  ${style.grey(`${included.length} files · secrets and conversation history excluded`)}`);
  return 0;
}
