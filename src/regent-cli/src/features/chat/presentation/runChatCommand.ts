// Runs a Regent CLI command from inside the chat by invoking the CLI itself as
// a subprocess (so every command + subcommand behaves exactly as in a terminal)
// and returns its plain-text output for a transcript note. Interactive or
// long-running commands are refused with a hint — they need a real terminal.
import { spawn } from "node:child_process";

// Interactive / terminal-owning commands can't run as a captured subprocess from
// inside the chat TUI: `call` opens a live voice call (LiveKit UI, needs a real
// TTY + browser), `setup` is a wizard, `mcp` serves on stdio, `chat` would nest.
const TERMINAL_ONLY = new Set(["chat", "setup", "mcp", "call"]);

export function runChatCommand(home: string, raw: string, onDone: (text: string) => void): void {
  const line = raw.trim().replace(/^regent\s+/i, "");
  const parts = line.split(/\s+/).filter(Boolean);
  const cmd = (parts[0] ?? "").toLowerCase();
  if (!cmd) {
    onDone("type a command — e.g. /status, /kanban list, /insights");
    return;
  }
  if (
    TERMINAL_ONLY.has(cmd) ||
    parts.includes("edit") ||
    parts.includes("-f") ||
    parts.includes("--follow")
  ) {
    onDone(
      `\`${line}\` is interactive or long-running — run it in a terminal. Read-only commands work here (e.g. /status, /kanban list, /insights, /tools).`,
    );
    return;
  }

  // In the compiled binary, execPath IS the CLI; in dev (`bun run`) it's bun, so
  // re-invoke the entry script. NO_COLOR keeps the captured output clean.
  const compiled = process.env.DEV === "false";
  const argv = compiled ? parts : [process.argv[1] ?? "", ...parts];
  let buf = "";
  try {
    const child = spawn(process.execPath, argv, {
      env: { ...process.env, REGENT_HOME: home, NO_COLOR: "1" },
    });
    const cap = (d: Buffer) => {
      buf += d.toString();
    };
    child.stdout?.on("data", cap);
    child.stderr?.on("data", cap);
    const timer = setTimeout(() => child.kill(), 30_000);
    child.on("close", () => {
      clearTimeout(timer);
      onDone(buf.trim() || "(no output)");
    });
    child.on("error", (e) => {
      clearTimeout(timer);
      onDone(`failed to run: ${e.message}`);
    });
  } catch (e) {
    onDone(`failed to run: ${e instanceof Error ? e.message : String(e)}`);
  }
}
