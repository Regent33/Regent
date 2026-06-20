// ANSI styling for one-shot (non-Ink) command output, derived from the palette.
// Disabled when stdout isn't a TTY or NO_COLOR is set, so piped output is clean.
import { palette } from "@shared/ui/tokens/theme.ts";

const enabled = process.stdout.isTTY === true && !process.env.NO_COLOR;

function rgb(hex: string): string {
  const h = hex.replace("#", "");
  const r = Number.parseInt(h.slice(0, 2), 16);
  const g = Number.parseInt(h.slice(2, 4), 16);
  const b = Number.parseInt(h.slice(4, 6), 16);
  return `38;2;${r};${g};${b}`;
}

function paint(codes: string[], s: string): string {
  return enabled ? `\x1b[${codes.join(";")}m${s}\x1b[0m` : s;
}

export const style = {
  teal: (s: string) => paint([rgb(palette.teal)], s),
  gold: (s: string) => paint([rgb(palette.gold)], s),
  grey: (s: string) => paint([rgb(palette.grey)], s),
  bold: (s: string) => paint(["1"], s),
  heading: (s: string) => paint(["1", rgb(palette.teal)], s),
  value: (s: string) => paint(["1", "97"], s),
  pass: (s: string) => paint([rgb("#3FB950")], s),
  warn: (s: string) => paint([rgb(palette.gold)], s),
  fail: (s: string) => paint([rgb("#F85149")], s),
} as const;
