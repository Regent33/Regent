// `regent voice serve` — one button for the local Qwen3 speech server. Finds a
// Python, checks the deps, prints the (2-step) install if they're missing, else
// launches scripts/local_speech_server.py in the foreground (Ctrl-C stops it).
// The manual "run this python script + fight pip" dance, collapsed to one command.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { out, printError } from "@app/cli/runtime.ts";
import { style } from "@shared/ui/style.ts";

const SCRIPT = join("scripts", "local_speech_server.py");
// qwen-asr & qwen-tts pin different transformers builds, so they can't co-resolve
// in one `pip install` — install ASR's stack, then TTS with --no-deps (kept .6).
const INSTALL = [
  "pip install qwen-asr soundfile librosa torchaudio sox einops",
  "pip install --no-deps qwen-tts",
];
// find_spec, not import — checking presence without paying torch's import cost.
const DEP_CHECK =
  "import importlib.util,sys;sys.exit(0 if all(importlib.util.find_spec(m) for m in ('soundfile','qwen_asr','qwen_tts')) else 1)";

// Try `python`, then the Windows `py -3` launcher, then `python3`. Returns the
// interpreter split as [binary, leading-args] (e.g. ["py", ["-3"]]).
function findPython(): [string, string[]] | null {
  for (const [bin, ...rest] of [["python"], ["py", "-3"], ["python3"]] as const) {
    if (spawnSync(bin, [...rest, "--version"], { stdio: "ignore" }).status === 0)
      return [bin, [...rest]];
  }
  return null;
}

export function voiceServe(): number {
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
    printError("local speech deps aren't installed yet — run these two (order matters):");
    for (const cmd of INSTALL) out(`    ${style.teal(cmd)}`);
    out(style.grey("  (the 2 steps avoid a transformers version clash between qwen-asr/qwen-tts)"));
    return 1;
  }
  out(`${style.pass("✓")} starting local speech server ${style.grey("— Ctrl-C to stop")}`);
  const run = spawnSync(bin, [...pre, SCRIPT], { stdio: "inherit" });
  return run.status ?? 0;
}
