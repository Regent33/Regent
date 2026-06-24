// `regent call serve` — one command for the live Jarvis call UI. Ensures the web
// app's deps are installed, seeds a .env.local (LiveKit dev defaults) on first
// run, reports how to bring up the LiveKit server + agent brain, then launches
// the Next.js UI in the foreground (Ctrl-C stops it). The "clone the repo, fight
// next/npm, find the URL" dance collapsed to one command — like `voice serve`.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { style } from "@shared/ui/style.ts";

const WEB_DIR = join("src", "regent-web");

export function callServe(): number {
  if (!existsSync(join(WEB_DIR, "package.json"))) {
    printError(`can't find ${WEB_DIR} — run this from the Regent repo root.`);
    return 1;
  }
  if (spawnSync("bun", ["--version"], { stdio: "ignore" }).status !== 0) {
    printError("bun not found on PATH — install Bun (https://bun.sh) and re-run.");
    return 1;
  }

  // First run: install the web deps (Next, LiveKit, three.js, …).
  if (!existsSync(join(WEB_DIR, "node_modules"))) {
    out(`${style.teal("installing web deps…")} ${style.grey("(first run only)")}`);
    if (spawnSync("bun", ["install"], { cwd: WEB_DIR, stdio: "inherit" }).status !== 0) {
      printError("`bun install` failed in the web app.");
      return 1;
    }
  }

  // Seed .env.local from the example so the token route + client have config.
  const envLocal = join(WEB_DIR, ".env.local");
  if (!existsSync(envLocal) && existsSync(join(WEB_DIR, ".env.example"))) {
    copyFileSync(join(WEB_DIR, ".env.example"), envLocal);
    out(
      `${style.grey("created")} ${envLocal} ${style.grey("— LiveKit dev defaults; edit for Cloud")}`,
    );
  }

  preflight(envLocal);

  out(`${style.pass("✓")} starting Regent live-call UI ${style.grey("— Ctrl-C to stop")}`);
  out(`  ${style.teal("call:")} http://localhost:3000`);
  return spawnSync("bun", ["run", "dev"], { cwd: WEB_DIR, stdio: "inherit" }).status ?? 0;
}

// Read .env.local and tell the user exactly what's needed for a *full* duplex
// call (vs. the local-mic preview the UI always gives). No call is made; this is
// guidance, not a health check.
function preflight(envLocal: string): void {
  const env = parseEnv(envLocal);
  const url = env.NEXT_PUBLIC_LIVEKIT_URL ?? "";
  const keyed = !!env.LIVEKIT_API_KEY && !!env.LIVEKIT_API_SECRET;
  if (!url || !keyed) {
    out(
      `  ${style.warn("LiveKit not fully configured")} ${style.grey("— UI runs in local-mic preview.")}`,
    );
    return;
  }
  if (url.includes("localhost") || url.includes("127.0.0.1")) {
    out(`  ${style.grey("LiveKit (self-host) — start the server with:")}`);
    out(`    ${style.teal("docker run --rm -p 7880:7880 -p 7881:7881 -p 7882:7882/udp \\")}`);
    out(`    ${style.teal("  livekit/livekit-server --dev")}`);
  } else {
    out(`  ${style.grey(`LiveKit: ${url}`)}`);
  }
  out(
    `  ${style.grey("Agent brain: run the Rust agent built with")} ${style.teal("--features livekit")} ${style.grey("(+ OPENAI_API_KEY). See ADR-021.")}`,
  );
}

// Minimal .env parser (KEY=VALUE, # comments, optional quotes) — mirrors the
// gateway/voice readers; we only need a few keys, no dotenv dependency.
function parseEnv(path: string): Record<string, string> {
  const env: Record<string, string> = {};
  let raw = "";
  try {
    raw = readFileSync(path, "utf8");
  } catch {
    return env;
  }
  for (const line of raw.split("\n")) {
    const t = line.trim();
    const eq = t.indexOf("=");
    if (!t || t.startsWith("#") || eq <= 0) continue;
    env[t.slice(0, eq).trim()] = t
      .slice(eq + 1)
      .trim()
      .replace(/^"|"$/g, "");
  }
  return env;
}
