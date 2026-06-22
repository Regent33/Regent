import { palette } from "@shared/ui/tokens/theme.ts";
// A rounded teal panel with the title set into the top border. Ink's Box
// can't title a border, so we draw the top edge by hand and
// give the body box every edge but the top — they line up at a shared width.
import { Box, Text, useStdout } from "ink";
import type { ReactNode } from "react";

interface PanelProps {
  readonly title: string;
  /** Total panel width in columns; clamped to the terminal. Defaults to 64. */
  readonly width?: number;
  readonly children: ReactNode;
}

export function Panel({ title, width = 64, children }: PanelProps) {
  const { stdout } = useStdout();
  const cols = stdout?.columns ?? 80;
  const w = Math.max(title.length + 6, Math.min(width, cols - 1));
  // Top edge: "╭─ <title> " + fill + "╮", total length = w.
  const fill = "─".repeat(Math.max(0, w - title.length - 5));

  return (
    <Box flexDirection="column" width={w}>
      <Text color={palette.teal}>
        {"╭─ "}
        <Text bold color={palette.white}>
          {title}
        </Text>
        {` ${fill}╮`}
      </Text>
      <Box
        flexDirection="column"
        width={w}
        borderStyle="round"
        borderTop={false}
        borderColor={palette.teal}
        paddingX={1}
      >
        {children}
      </Box>
    </Box>
  );
}
