// An animated braille spinner for one-shot console commands (not the Ink TUI).
// Mirrors the TUI Spinner's frames; renders in place on stderr. When stderr
// isn't a TTY (piped, or run from inside chat as a subprocess) it animates
// nothing and just runs `fn` — so captured output stays clean.
import { style } from "@shared/ui/style.ts";

const FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] as const;

export async function withSpinner<T>(label: string, fn: () => Promise<T>): Promise<T> {
  if (!process.stderr.isTTY) return fn();
  let i = 0;
  const render = (): void => {
    process.stderr.write(
      `\r  ${style.teal(FRAMES[i % FRAMES.length] ?? "⠋")} ${style.grey(label)}`,
    );
    i += 1;
  };
  render();
  const id = setInterval(render, 80);
  try {
    return await fn();
  } finally {
    clearInterval(id);
    // Wipe the spinner line so the next output starts clean.
    process.stderr.write(`\r${" ".repeat(label.length + 6)}\r`);
  }
}
