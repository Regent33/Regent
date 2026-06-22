import type { SlashCommand } from "@app/config/commands.ts";
import { palette } from "@shared/ui/tokens/theme.ts";
// The `/` autocomplete menu — modelled on Claude Code's PromptInput suggestions:
// each row is the command name in a left column padded to ~40% of the terminal
// width, then a dim description (whitespace collapsed) filling the rest on one
// truncated line. Selection is shown by colour — the selected row is teal/bold,
// the others dim — exactly as Claude Code marks `selectedSuggestion`.
import { Box, Text } from "ink";

const MAX_ROWS = 8;

interface CommandMenuProps {
  readonly items: readonly SlashCommand[];
  readonly selected: number;
}

export function CommandMenu({ items, selected }: CommandMenuProps) {
  if (items.length === 0) return null;

  // Scroll the visible window so the selected row stays in view.
  let start = 0;
  if (items.length > MAX_ROWS) {
    start = Math.min(Math.max(0, selected - Math.floor(MAX_ROWS / 2)), items.length - MAX_ROWS);
  }
  const window = items.slice(start, start + MAX_ROWS);

  const columns = process.stdout.columns ?? 80;
  // Name column ≈ 40% of width (Claude Code's `maxNameWidth`), but no wider than
  // the longest visible name; the description fills what's left.
  const longest = Math.max(...window.map((c) => c.name.length + 1));
  const nameW = Math.min(longest + 1, Math.floor(columns * 0.4));
  const descW = Math.max(8, columns - nameW - 4);

  return (
    <Box flexDirection="column" marginBottom={1}>
      {window.map((c, i) => {
        const on = start + i === selected;
        return (
          <Text key={c.name} color={on ? palette.teal : palette.grey} bold={on} wrap="truncate">
            {`/${c.name}`.padEnd(nameW)}
            {truncate(c.description.replace(/\s+/g, " "), descW)}
          </Text>
        );
      })}
    </Box>
  );
}

function truncate(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, Math.max(0, max - 1))}…` : s;
}
