import { palette } from "@shared/ui/tokens/theme.ts";
// Frames an agent reply in a full rounded box with the label set into the top
// border — the way the Hermes CLI brackets its replies. Ink's Box can't title a
// border, so the top edge is drawn by hand and the body box gets every edge but
// the top; they line up at a shared width. Top: "╭─ ✦ Regent ──╮", bottom "╰──╯".
import { Box, Text } from "ink";
import type { ReactNode } from "react";

const LABEL = "✦ Regent";

export function AssistantFrame({ children }: { readonly children: ReactNode }) {
  // Sized once at render — committed history doesn't reflow on resize (the TUI
  // resize model), so we read the launch width like the brand header does.
  const cols = Math.max(LABEL.length + 8, (process.stdout.columns ?? 80) - 2);
  // Top edge: "╭─ " (3) + label + " " (1) + fill + "╮" (1) = cols.
  const fill = "─".repeat(Math.max(0, cols - LABEL.length - 5));
  return (
    <Box flexDirection="column" width={cols} marginY={1}>
      <Text color={palette.teal}>
        {"╭─ "}
        <Text bold color={palette.gold}>
          {LABEL}
        </Text>
        {` ${fill}╮`}
      </Text>
      <Box
        flexDirection="column"
        width={cols}
        borderStyle="round"
        borderTop={false}
        borderColor={palette.teal}
        paddingX={2}
      >
        {children}
      </Box>
    </Box>
  );
}
