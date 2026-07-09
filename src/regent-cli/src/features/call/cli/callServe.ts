// `regent call serve` — one command for the live Jarvis call UI. Auto-starts the
// local speech backend (so you don't run `voice serve` separately), ensures the
// web app's deps are installed, seeds a .env.local on first run, launches the
// Next.js UI, and opens the browser when it's ready (Ctrl-C stops the UI).
import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, readFileSync } from "node:fs";
import { get } from "node:http";
import { dirname, join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import {
  hasRustServer,
  speechDepsOk,
  speechServerUp,
  speechServerWarm,
  startSpeechServerDetached,
} from "@features/voice/cli/voiceServe.ts";
import { style } from "@shared/ui/style.ts";

const delay = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

// Locate <repo>/src/regent-web so `regent call serve` works from ANY directory:
// an explicit REGENT_WEB_DIR override, then the cwd, the running binary's dir
// (the compiled `regent` lives in the repo at src/regent-cli/dist), and finally
// this source file's dir (dev mode). Each start point walks up to a parent that
// contains src/regent-web.
function findWebDir(): string | null {
  const direct = process.env.REGENT_WEB_DIR;
  if (direct && existsSync(join(direct, "package.json"))) return direct;
  for (const start of [direct, process.cwd(), dirname(process.execPath), import.meta.dir]) {
    if (!start) continue;
    let dir = start;
    for (let i = 0; i < 12; i++) {
      const candidate = join(dir, "src", "regent-web");
      if (existsSync(join(candidate, "package.json"))) return candidate;
      const parent = dirname(dir);
      if (parent === dir) break; // reached the filesystem root
      dir = parent;
    }
  }
  return null;
}

export async function callServe(profile: string): Promise<number> {
  const webDir = findWebDir();
  if (!webDir) {
    printError(
      "can't find src/regent-web — run from inside the Regent repo, or set REGENT_WEB_DIR to its path.",
    );
    return 1;
  }
  if (spawnSync("bun", ["--version"], { stdio: "ignore" }).status !== 0) {
    printError("bun not found on PATH — install Bun (https://bun.sh) and re-run.");
    return 1;
  }

  // First run: install the web deps (Next, LiveKit, three.js, …).
  if (!existsSync(join(webDir, "node_modules"))) {
    out(`${style.teal("installing web deps…")} ${style.grey("(first run only)")}`);
    if (spawnSync("bun", ["install"], { cwd: webDir, stdio: "inherit" }).status !== 0) {
      printError("`bun install` failed in the web app.");
      return 1;
    }
  }

  // Seed .env.local from the example so the token route + client have config.
  const envLocal = join(webDir, ".env.local");
  if (!existsSync(envLocal) && existsSync(join(webDir, ".env.example"))) {
    copyFileSync(join(webDir, ".env.example"), envLocal);
    out(
      `${style.grey("created")} ${envLocal} ${style.grey("— LiveKit dev defaults; edit for Cloud")}`,
    );
  }

  // Auto-start the local speech backend (ASR/TTS) so the call works without a
  // separate `regent voice serve`. Detached + reused: started once, left running.
  await ensureSpeechBackend(profile);

  preflight(envLocal);

  out(`${style.pass("✓")} starting Regent live-call UI ${style.grey("— Ctrl-C to stop")}`);
  // Launch Next (non-blocking so we can open the browser once it's ready).
  const next = spawn("bun", ["run", "dev"], { cwd: webDir, stdio: "inherit" });
  void openWhenReady();
  return new Promise<number>((resolve) => {
    next.on("exit", (code) => resolve(code ?? 0));
    next.on("error", (e) => {
      printError(`failed to start the web UI: ${e.message}`);
      resolve(1);
    });
  });
}

// Bring up the speech server if it isn't already on :8000, then wait for its
// models to WARM — the cold first turn (15-25s) is what makes the call feel slow.
async function ensureSpeechBackend(profile: string): Promise<void> {
  if (await speechServerUp()) {
    out(`${style.pass("✓")} speech backend already running ${style.grey("(:8000)")}`);
  } else {
    const rust = hasRustServer();
    // Only the legacy Python fallback needs its deps preflighted.
    if (!rust && !speechDepsOk()) {
      out(
        `${style.warn("⚠ speech deps not installed")} ${style.grey("— run `regent voice serve` once to install, then retry.")}`,
      );
      return;
    }
    out(
      `${style.teal("starting speech backend…")} ${style.grey(rust ? "rust · whisper + Kokoro (ONNX)" : "python · faster-whisper + Kokoro")}`,
    );
    if (!startSpeechServerDetached(profile)) {
      out(
        `${style.warn("⚠ couldn't start the speech backend")} ${style.grey("— start it with `regent voice serve`.")}`,
      );
      return;
    }
    // Poll at 250ms (not 500) so the call connects as soon as the backend is up
    // — readiness is usually detected within one tick, halving the start lag.
    for (let i = 0; i < 100 && !(await speechServerUp()); i++) await delay(250);
  }

  if (await speechServerWarm()) {
    out(`${style.pass("✓")} speech backend warm ${style.grey("(:8000)")}`);
    return;
  }
  out(`${style.grey("  warming models (faster-whisper + Kokoro)…")}`);
  for (let i = 0; i < 120; i++) {
    if (await speechServerWarm()) {
      out(`${style.pass("✓")} speech backend warm ${style.grey("(:8000)")}`);
      return;
    }
    await delay(250); // 250ms tick (was 500) → warm detected sooner; ~30s budget kept
  }
  out(`${style.grey("  still warming — the first turn may be slow, then it's fast.")}`);
}

// Poll the likely Next ports and open the browser at the first one that answers.
async function openWhenReady(): Promise<void> {
  for (let i = 0; i < 120; i++) {
    for (const port of [3000, 3001, 3002]) {
      if (await httpOk(`http://localhost:${port}`)) {
        openBrowser(`http://localhost:${port}`);
        return;
      }
    }
    await delay(250); // 250ms tick (was 500) → open the UI as soon as it's ready
  }
}

function httpOk(url: string): Promise<boolean> {
  return new Promise((resolve) => {
    const req = get(url, (res) => {
      res.resume();
      resolve((res.statusCode ?? 500) < 500);
    });
    req.on("error", () => resolve(false));
    req.setTimeout(800, () => {
      req.destroy();
      resolve(false);
    });
  });
}

function openBrowser(url: string): void {
  out(`  ${style.teal("opening")} ${url}`);
  const [cmd, args] =
    process.platform === "win32"
      ? (["cmd", ["/c", "start", "", url]] as const)
      : process.platform === "darwin"
        ? (["open", [url]] as const)
        : (["xdg-open", [url]] as const);
  try {
    spawn(cmd, [...args], { stdio: "ignore", detached: true, windowsHide: true }).unref();
  } catch {
    // Browser auto-open is best-effort; the URL is printed above either way.
  }
}

// Read .env.local and tell the user exactly what's needed for a *full* duplex
// call (vs. the local-mic preview the UI always gives). No call is made; this is
// guidance, not a health check.
function preflight(envLocal: string): void {
  const env = parseEnv(envLocal);
  const url = env.NEXT_PUBLIC_LIVEKIT_URL ?? "";
  const keyed = !!env.LIVEKIT_API_KEY && !!env.LIVEKIT_API_SECRET;
  const liveKitOptIn =
    env.NEXT_PUBLIC_USE_LIVEKIT === "1" || env.NEXT_PUBLIC_USE_LIVEKIT === "true";
  if (!liveKitOptIn || !url || !keyed) {
    out(
      `  ${style.grey("local call mode — faster-whisper + Kokoro. (LiveKit is opt-in: NEXT_PUBLIC_USE_LIVEKIT=1)")}`,
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
