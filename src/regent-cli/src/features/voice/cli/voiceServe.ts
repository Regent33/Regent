// `regent voice serve` — one button for the local real-time speech server.
// Prefers the Rust server (regent-voice-server: whisper + Kokoro over ONNX,
// models auto-download on first run); falls back to the legacy Python server
// (python_server.py) when the Rust binary isn't built yet.
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { get } from "node:http";
import { dirname, join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { newestInTarget, regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

/** Health probe. Resolves the parsed body, or null if unreachable. */
function speechHealth(): Promise<{ warm?: boolean } | null> {
  return new Promise((resolve) => {
    const req = get("http://localhost:8000/health", (res) => {
      let body = "";
      res.on("data", (c) => {
        body += c;
      });
      res.on("end", () => {
        try {
          resolve(JSON.parse(body));
        } catch {
          resolve((res.statusCode ?? 500) < 500 ? {} : null);
        }
      });
    });
    req.on("error", () => resolve(null));
    req.setTimeout(800, () => {
      req.destroy();
      resolve(null);
    });
  });
}

/** True if the local speech server answers on :8000. */
export async function speechServerUp(): Promise<boolean> {
  return (await speechHealth()) !== null;
}

/** True once the server reports its models are warm (first call won't cold-load). */
export async function speechServerWarm(): Promise<boolean> {
  return (await speechHealth())?.warm === true;
}

/** True if the Rust speech server binary is available (preferred backend). */
export function hasRustServer(): boolean {
  return findRustServer() !== null;
}

/** True if the Python speech deps are importable. */
export function speechDepsOk(): boolean {
  const py = findPython();
  if (!py) return false;
  const [bin, pre] = py;
  return spawnSync(bin, [...pre, "-c", DEP_CHECK], { stdio: "ignore" }).status === 0;
}

/** Start the speech server detached so it survives + is reused across calls.
 *  Rust server first (ONNX engines, self-downloading models); Python fallback.
 *  Returns false if neither can be located. */
export function startSpeechServerDetached(profile: string): boolean {
  const rust = findRustServer();
  if (rust) {
    const child = spawn(rust.bin, [], {
      cwd: rust.cwd,
      env: brainEnv(profile),
      detached: true,
      stdio: "ignore",
      // No console window on Windows — a visible one invites the user to close
      // it, which kills the voice mid-call.
      windowsHide: true,
    });
    child.unref();
    return true;
  }
  const root = findRepoRoot();
  const py = findPython();
  if (!root || !py) return false;
  const [bin, pre] = py;
  const child = spawn(bin, [...pre, join(root, SCRIPT_REL)], {
    cwd: root,
    env: brainEnv(profile),
    detached: true,
    stdio: "ignore",
    windowsHide: true,
  });
  child.unref();
  return true;
}

const SCRIPT_REL = join("python-voice-server", "python_server.py");
const RUST_BIN = process.platform === "win32" ? "regent-voice-server.exe" : "regent-voice-server";

/** Locate the Rust speech server: REGENT_VOICE_SERVER_PATH, then
 *  target/{release,debug} walking up (same walk as the deacon), then next to
 *  the running binary. cwd = the target/'s parent so the default models dir
 *  (tts-asr-local-models) lands in the repo root like the Python server's. */
function findRustServer(): { bin: string; cwd: string } | null {
  const override = process.env.REGENT_VOICE_SERVER_PATH;
  if (override && existsSync(override)) return { bin: override, cwd: dirname(override) };
  for (const start of [
    process.env.REGENT_REPO_DIR,
    process.cwd(),
    dirname(process.execPath),
    import.meta.dir,
  ]) {
    if (!start) continue;
    let dir = start;
    for (let i = 0; i < 12; i++) {
      // Newest of release/debug wins — same staleness rule as the deacon walk.
      const cand = newestInTarget(dir, RUST_BIN);
      if (cand) return { bin: cand, cwd: dir };
      const parent = dirname(dir);
      if (parent === dir) break;
      dir = parent;
    }
  }
  return null;
}

// Find the repo root (the dir holding python-voice-server/python_server.py) so
// `regent voice serve` works from ANY directory — mirrors callServe/findWebDir
// and the deacon's walk-up. Start points: REGENT_REPO_DIR, cwd, the running
// binary's dir, this source file's dir; each walks up to a parent that has it.
function findRepoRoot(): string | null {
  for (const start of [
    process.env.REGENT_REPO_DIR,
    process.cwd(),
    dirname(process.execPath),
    import.meta.dir,
  ]) {
    if (!start) continue;
    let dir = start;
    for (let i = 0; i < 12; i++) {
      if (existsSync(join(dir, SCRIPT_REL))) return dir;
      const parent = dirname(dir);
      if (parent === dir) break; // filesystem root
      dir = parent;
    }
  }
  return null;
}
// Real-time stack: faster-whisper (CTranslate2 int8) ASR + Kokoro-82M TTS (Piper
// is the lighter fallback via REGENT_TTS_ENGINE=piper). For the GPU ASR path,
// also install the CUDA torch build (see python-voice-server/README.md).
const INSTALL = ["pip install faster-whisper kokoro-onnx soundfile"];
// find_spec, not import — checking presence without paying the import cost.
const DEP_CHECK =
  "import importlib.util,sys;sys.exit(0 if all(importlib.util.find_spec(m) for m in ('soundfile','faster_whisper','kokoro_onnx')) else 1)";

// Try `python`, then the Windows `py -3` launcher, then `python3`. Returns the
// interpreter split as [binary, leading-args] (e.g. ["py", ["-3"]]).
function findPython(): [string, string[]] | null {
  for (const [bin, ...rest] of [["python"], ["py", "-3"], ["python3"]] as const) {
    if (spawnSync(bin, [...rest, "--version"], { stdio: "ignore" }).status === 0)
      return [bin, [...rest]];
  }
  return null;
}

// Pass your configured model + key to the server so the call's brain is *Regent*
// (your model), not the echo fallback. Mirrors the gateway: .env for secrets,
// config.yaml for the model id. The real environment always wins.
function brainEnv(profile: string): NodeJS.ProcessEnv {
  const home = regentHome(profile);
  const env: NodeJS.ProcessEnv = { ...process.env };
  // The speech server may spawn an agent deacon (agentic voice); point it at this
  // profile's home so it uses the right memory/persona/sessions.
  if (env.REGENT_HOME === undefined) env.REGENT_HOME = home;
  try {
    for (const raw of readFileSync(join(home, ".env"), "utf8").split("\n")) {
      const line = raw.trim();
      const eq = line.indexOf("=");
      if (!line || line.startsWith("#") || eq <= 0) continue;
      const key = line.slice(0, eq).trim();
      if (env[key] === undefined)
        env[key] = line
          .slice(eq + 1)
          .trim()
          .replace(/^"|"$/g, "");
    }
  } catch {
    // no .env — brain falls back to echo, which is fine
  }
  try {
    const cfg = YAML.parse(readFileSync(join(home, "config.yaml"), "utf8")) as {
      model?: { default?: string; base_url?: string };
    } | null;
    if (cfg?.model?.default && !env.REGENT_MODEL) env.REGENT_MODEL = cfg.model.default;
    if (cfg?.model?.base_url && !env.REGENT_BASE_URL) env.REGENT_BASE_URL = cfg.model.base_url;
  } catch {
    // no config.yaml — same
  }
  return env;
}

export function voiceServe(profile: string): number {
  // Rust server first: whisper + Kokoro over ONNX, single binary, models
  // auto-download on first run. Build: cargo build -p regent-voice-server --release
  const rust = findRustServer();
  if (rust) {
    out(
      `${style.pass("✓")} starting local speech server ${style.grey("(rust · onnx) — Ctrl-C to stop")}`,
    );
    out(style.grey("  voice call: http://localhost:8000/call"));
    const run = spawnSync(rust.bin, [], {
      stdio: "inherit",
      cwd: rust.cwd,
      env: brainEnv(profile),
    });
    return run.status ?? 0;
  }
  out(
    style.grey(
      "regent-voice-server binary not found (cargo build -p regent-voice-server --release) — using the Python server",
    ),
  );
  const root = findRepoRoot();
  if (!root) {
    printError(
      `can't find ${SCRIPT_REL} — run from inside the Regent repo, or set REGENT_REPO_DIR to its path.`,
    );
    return 1;
  }
  const py = findPython();
  if (!py) {
    printError("no Python found on PATH — install Python 3.10+ and re-run.");
    return 1;
  }
  const [bin, pre] = py;
  if (spawnSync(bin, [...pre, "-c", DEP_CHECK], { stdio: "ignore" }).status !== 0) {
    printError("local speech deps aren't installed yet — run:");
    for (const cmd of INSTALL) out(`    ${style.teal(cmd)}`);
    out(style.grey("  (real-time stack: faster-whisper + Kokoro; both have Python 3.14 wheels)"));
    return 1;
  }
  out(`${style.pass("✓")} starting local speech server ${style.grey("— Ctrl-C to stop")}`);
  out(style.grey("  voice call: http://localhost:8000/call"));
  // cwd = repo root so the server's default REGENT_MODELS_DIR ("tts-asr-local-models")
  // resolves regardless of where `regent voice serve` was invoked from.
  const script = join(root, SCRIPT_REL);
  const run = spawnSync(bin, [...pre, script], {
    stdio: "inherit",
    cwd: root,
    env: brainEnv(profile),
  });
  return run.status ?? 0;
}
