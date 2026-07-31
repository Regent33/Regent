// The chat surface is an Ink app: it needs a real terminal on both ends. Piped
// into, it used to paint the banner and then hang forever (audit probe 10), and
// on a fresh home it hung inside the onboarding wizard instead. The check lives
// here so the router can run it before Ink initialises and before the deacon is
// spawned — a guard that fires after either can still orphan a daemon.
import { EXIT } from "@app/cli/exit.ts";
import { err } from "@app/cli/runtime.ts";
import { style } from "@shared/ui/style.ts";

/**
 * Returns null when it is safe to open the interactive chat, otherwise the exit
 * code to return. Never reads stdin: an open pipe that never sends data must
 * not block, so the decision is made from the descriptors alone.
 */
/** Every route that opens the Ink chat surface, named in one place. */
export function opensChat(command: string, args: readonly string[]): boolean {
  if (command === "" || command === "chat") return true;
  return command === "sessions" && args[0] === "resume";
}

export function refuseNonInteractive(): number | null {
  // Terminals that misreport (some Git Bash / mintty and mux setups) get an
  // escape hatch rather than an unusable CLI.
  if (process.env.REGENT_FORCE_TTY === "1") return null;
  if (process.stdin.isTTY === true && process.stdout.isTTY === true) return null;

  const which = process.stdin.isTTY !== true ? "stdin" : "stdout";
  err(`${style.fail("✗")} regent chat needs an interactive terminal — ${which} is not a terminal.`);
  err("");
  err("  Piping into `regent` is not supported: there is no one-shot mode yet.");
  err('  For a scripted task use:  regent code "<task>"');
  err("  Misdetected terminal? Set REGENT_FORCE_TTY=1.");
  return EXIT.usage;
}
