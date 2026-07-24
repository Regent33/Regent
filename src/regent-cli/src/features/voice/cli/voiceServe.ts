// `regent voice serve` — one button for the local real-time speech server.
// Prefers the Rust server (regent-voice-server: whisper + Kokoro over ONNX,
// models auto-download on first run); falls back to the legacy Python server
// (python_server.py) when the Rust binary isn't built yet.
// Binary/interpreter location and the brain env live in voiceBackend.ts.
import { spawn, spawnSync } from "node:child_process";
import { get } from "node:http";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { style } from "@shared/ui/style.ts";
import {
  DEP_CHECK,
  INSTALL,
  RUST_BIN,
  SCRIPT_REL,
  brainEnv,
  findPython,
  findRepoRoot,
  findRustServer,
} from "./voiceBackend.ts";

export { voiceModelSelection, type VoiceModelSelection } from "./voiceBackend.ts";

// Must match the Rust voice server's `CALL_PROTOCOL`
// (src/crates/regent-voice-server/src/infra/http/pages.rs) and the Desktop
// constant (src/regent-app/Desktop/shared/infrastructure/voice/protocol.ts).
// Kept honest by scripts/tests/verify-versions.py in CI.
export const CALL_PROTOCOL = 7;

interface SpeechHealth {
  engine?: string;
  call_protocol?: number;
  warm?: boolean;
}

export type SpeechServerState = "current" | "stale" | "down";

/** Only Rust servers use the versioned Butler contract. The supported legacy
 * Python backend has a different engine identity and remains usable. */
export function classifySpeechHealth(health: SpeechHealth | null): SpeechServerState {
  if (health === null) return "down";
  if (health.engine?.startsWith("regent-voice-server")) {
    return health.call_protocol === CALL_PROTOCOL ? "current" : "stale";
  }
  return "current";
}

/** Health probe. Resolves the parsed body, or null if unreachable. */
function speechHealth(): Promise<SpeechHealth | null> {
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
  return (await speechServerState()) === "current";
}

/** Distinguish unreachable, compatible, and a reused old Rust call contract. */
export async function speechServerState(): Promise<SpeechServerState> {
  return classifySpeechHealth(await speechHealth());
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

/** Stop only the known Rust voice process after its health contract proved it
 * stale. The caller waits briefly for port 8000 before starting the new one. */
export function stopStaleRustServer(): void {
  const [bin, args] =
    process.platform === "win32"
      ? (["taskkill", ["/F", "/IM", RUST_BIN]] as const)
      : (["pkill", ["-x", RUST_BIN]] as const);
  spawnSync(bin, [...args], { stdio: "ignore", windowsHide: true });
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
