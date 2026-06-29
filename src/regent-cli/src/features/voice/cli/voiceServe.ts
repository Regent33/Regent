// `regent voice serve` — one button for the local real-time speech server
// (faster-whisper ASR + Piper TTS). Finds a Python, checks the deps, prints the
// install if they're missing, else launches python-voice-server/python_server.py
// in the foreground (Ctrl-C stops it).
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/daemon/locate.ts";
import { style } from "@shared/ui/style.ts";
import YAML from "yaml";

const SCRIPT = join("python-voice-server", "python_server.py");
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
  if (!existsSync(SCRIPT)) {
    printError(`can't find ${SCRIPT} — run this from the Regent repo root.`);
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
    out(style.grey("  (real-time stack: faster-whisper + Piper; both have Python 3.14 wheels)"));
    return 1;
  }
  out(`${style.pass("✓")} starting local speech server ${style.grey("— Ctrl-C to stop")}`);
  out(style.grey("  voice call: http://localhost:8000/call"));
  const run = spawnSync(bin, [...pre, SCRIPT], { stdio: "inherit", env: brainEnv(profile) });
  return run.status ?? 0;
}
