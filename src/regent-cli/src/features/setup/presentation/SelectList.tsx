import { palette } from "@shared/ui/tokens/theme.ts";
// Windowed arrow-key list rows (render-only — the parent owns useInput),
// styled like the chat TUI's CommandMenu: selected row teal/bold, rest dim,
// window scrolled so the selection stays visible.
import { Box, Text } from "ink";

const MAX_ROWS = 10;

export interface SelectRow {
  readonly label: string;
  readonly hint?: string;
}

interface SelectListProps {
  readonly rows: readonly SelectRow[];
  readonly selected: number;
}

export function SelectList({ rows, selected }: SelectListProps) {
  if (rows.length === 0) return null;

  let start = 0;
  if (rows.length > MAX_ROWS) {
    start = Math.min(Math.max(0, selected - Math.floor(MAX_ROWS / 2)), rows.length - MAX_ROWS);
  }
  const window = rows.slice(start, start + MAX_ROWS);

  const columns = process.stdout.columns ?? 80;
  const longest = Math.max(...window.map((r) => r.label.length + 1));
  const labelW = Math.min(longest + 2, Math.floor(columns * 0.4));
  const hintW = Math.max(8, columns - labelW - 6);

  return (
    <Box flexDirection="column">
      {start > 0 && <Text color={palette.grey}> ↑ more</Text>}
      {window.map((r, i) => {
        const on = start + i === selected;
        return (
          <Text key={r.label} color={on ? palette.teal : palette.grey} bold={on} wrap="truncate">
            {(on ? "❯ " : "  ") + r.label.padEnd(labelW)}
            {truncate(r.hint ?? "", hintW)}
          </Text>
        );
      })}
      {start + MAX_ROWS < rows.length && <Text color={palette.grey}> ↓ more</Text>}
    </Box>
  );
}

function truncate(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, Math.max(0, max - 1))}…` : s;
}
