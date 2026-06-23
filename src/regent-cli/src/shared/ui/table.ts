// A terminal-width-aware box table for one-shot (non-Ink) command output.
// Columns size to their content; one `flex` column absorbs the leftover width
// and truncates with `…`, so the table never overflows the terminal and adapts
// when it's resized. Cells are sized on their *visible* width (ANSI-stripped),
// then painted — so colour never breaks alignment.
import { style } from "@shared/ui/style.ts";

export interface Column<T> {
  /** Column heading. */
  readonly header: string;
  /** Raw (uncoloured) cell text for a row — paint via `paint`, not here. */
  readonly get: (row: T) => string;
  /** Optional colour, applied after sizing. Gets the padded cell + the row. */
  readonly paint?: (cell: string, row: T) => string;
  /** Right-align (numbers/dates); default left. */
  readonly align?: "right";
  /** This column soaks up the leftover terminal width and truncates. One max. */
  readonly flex?: boolean;
  /** Floor for this column's width (default: header length). */
  readonly min?: number;
}

// Build the SGR-sequence matcher without a literal ESC in the regex source
// (biome forbids control characters in regex literals).
const ANSI = new RegExp(`${String.fromCharCode(27)}\\[[0-9;]*m`, "g");

/** Width of `s` as the terminal renders it (colour codes don't take columns). */
export const visibleWidth = (s: string): number => s.replace(ANSI, "").length;

/** Truncate (with …) or pad `s` to exactly `w` visible columns. */
function fit(s: string, w: number, align?: "right"): string {
  const len = visibleWidth(s);
  if (len > w) return w <= 1 ? "…" : `${s.slice(0, w - 1)}…`;
  const padding = " ".repeat(w - len);
  return align === "right" ? padding + s : s + padding;
}

/** Render `rows` as box-table lines. Caller handles the empty case + heading. */
export function renderTable<T>(rows: readonly T[], cols: readonly Column<T>[]): string[] {
  const term = process.stdout.columns && process.stdout.columns > 0 ? process.stdout.columns : 100;

  const layout = cols.map((col) => ({
    col,
    width: Math.max(visibleWidth(col.header), col.min ?? 0, ...rows.map((r) => visibleWidth(col.get(r)))),
  }));

  // Row overhead = "│ " + … + " │ " between + " │" = 3·cols + 1 visible columns.
  const overhead = 3 * cols.length + 1;
  const flex = layout.find((l) => l.col.flex);
  if (flex) {
    const others = layout.reduce((sum, l) => (l === flex ? sum : sum + l.width), 0);
    const avail = term - overhead - others;
    flex.width = Math.max(flex.col.min ?? 8, Math.min(flex.width, avail));
  }

  const rule = (l: string, mid: string, r: string): string =>
    style.grey(l + layout.map((x) => "─".repeat(x.width + 2)).join(mid) + r);
  const rowLine = (cells: string[]): string =>
    `${style.grey("│ ")}${cells.join(style.grey(" │ "))}${style.grey(" │")}`;

  const lines: string[] = [rule("╭", "┬", "╮")];
  lines.push(rowLine(layout.map((x) => style.bold(fit(x.col.header, x.width, x.col.align)))));
  lines.push(rule("├", "┼", "┤"));
  for (const r of rows) {
    lines.push(
      rowLine(
        layout.map((x) => {
          const cell = fit(x.col.get(r), x.width, x.col.align);
          return x.col.paint ? x.col.paint(cell, r) : cell;
        }),
      ),
    );
  }
  lines.push(rule("╰", "┴", "╯"));
  return lines;
}
