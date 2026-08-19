// Backend plumbing for the local speech server: locating the Rust binary /
// Python fallback, and building the child environment that carries the
// configured model + keys (the call's "brain").
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { newestInTarget, regentHome } from "@shared/infrastructure/deacon/locate.ts";
import YAML from "yaml";

export const SCRIPT_REL = join("python-voice-server", "python_server.py");
export const RUST_BIN =
  process.platform === "win32" ? "regent-voice-server.exe" : "regent-voice-server";

// Real-time stack: faster-whisper (CTranslate2 int8) ASR + Kokoro-82M TTS (Piper
// is the lighter fallback via REGENT_TTS_ENGINE=piper). For the GPU ASR path,
// also install the CUDA torch build (see python-voice-server/README.md).
export const INSTALL = ["pip install faster-whisper kokoro-onnx soundfile"];
// find_spec, not import — checking presence without paying the import cost.
export const DEP_CHECK =
  "import importlib.util,sys;sys.exit(0 if all(importlib.util.find_spec(m) for m in ('soundfile','faster_whisper','kokoro_onnx')) else 1)";

export interface VoiceModelConfig {
  model?: { default?: string; base_url?: string };
  agents_defaults?: { primary?: { provider?: string; model?: string } };
  speech?: { call?: { fast_model?: string } };
}

export interface VoiceModelSelection {
  readonly model: string;
  readonly baseUrl?: string;
}

/** Resolve the same qualified provider/model persisted by main chat. A
 * voice-only fast model is the sole override; the legacy default is fallback. */
export function voiceModelSelection(cfg: VoiceModelConfig | null): VoiceModelSelection | undefined {
  const fast = cfg?.speech?.call?.fast_model?.trim();
  if (fast) return { model: fast };
  const provider = cfg?.agents_defaults?.primary?.provider?.trim();
  const model = cfg?.agents_defaults?.primary?.model?.trim();
  if (provider && model) return { model: `${provider}/${model}` };
  const legacy = cfg?.model?.default?.trim();
  if (!legacy) return undefined;
  const baseUrl = cfg?.model?.base_url?.trim();
  return { model: legacy, ...(baseUrl ? { baseUrl } : {}) };
}

/** Locate the Rust speech server: REGENT_VOICE_SERVER_PATH, then NEXT TO the
 *  running binary, then target/{release,debug} walking up (same walk as the
 *  deacon).
 *
 *  The sibling check is not a nicety — this comment already claimed it and the
 *  code did not do it, so an INSTALLED Regent never found the server at all:
 *  an install has no target/ directory, and the walk only ever looked there.
 *
 *  cwd decides where the default models dir (tts-asr-local-models) lands. A
 *  repo build keeps putting it at the repo root as before; an installed binary
 *  gets REGENT_HOME, so the whisper + Kokoro bundles live with the rest of the
 *  user's data instead of inside the install folder. */
export function findRustServer(profile = ""): { bin: string; cwd: string } | null {
  const override = process.env.REGENT_VOICE_SERVER_PATH;
  if (override && existsSync(override)) return { bin: override, cwd: dirname(override) };
  const beside = join(dirname(process.execPath), RUST_BIN);
  if (existsSync(beside)) return { bin: beside, cwd: regentHome(profile) };
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
export function findRepoRoot(): string | null {
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

// Try `python`, then the Windows `py -3` launcher, then `python3`. Returns the
// interpreter split as [binary, leading-args] (e.g. ["py", ["-3"]]).
export function findPython(): [string, string[]] | null {
  for (const [bin, ...rest] of [["python"], ["py", "-3"], ["python3"]] as const) {
    if (spawnSync(bin, [...rest, "--version"], { stdio: "ignore" }).status === 0)
      return [bin, [...rest]];
  }
  return null;
}

// Pass your configured model + key to the server so the call's brain is *Regent*
// (your model), not the echo fallback. Mirrors the gateway: .env for secrets,
// config.yaml for the model id. The real environment always wins.
export function brainEnv(profile: string): NodeJS.ProcessEnv {
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
    const cfg = YAML.parse(
      readFileSync(join(home, "config.yaml"), "utf8"),
    ) as VoiceModelConfig | null;
    const selected = voiceModelSelection(cfg);
    if (selected && !env.REGENT_MODEL) env.REGENT_MODEL = selected.model;
    if (selected?.baseUrl && !env.REGENT_BASE_URL) env.REGENT_BASE_URL = selected.baseUrl;
  } catch {
    // no config.yaml — same
  }
  return env;
}
