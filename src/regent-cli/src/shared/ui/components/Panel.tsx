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

export interface PanelLayout {
  readonly title: string;
  readonly width: number;
}

/** Clamp the whole outline to the terminal and clip its border title with it. */
export function panelLayout(title: string, requested: number, columns: number): PanelLayout {
  const width = Math.max(1, Math.min(Math.max(6, requested), Math.max(1, columns - 1)));
  const titleWidth = Math.max(0, width - 5);
  const fittedTitle =
    title.length <= titleWidth
      ? title
      : titleWidth <= 1
        ? "…".slice(0, titleWidth)
        : `${title.slice(0, titleWidth - 1)}…`;
  return { title: fittedTitle, width };
}

export function Panel({ title, width = 64, children }: PanelProps) {
  const { stdout } = useStdout();
  const layout = panelLayout(title, width, stdout?.columns ?? 80);
  // Top edge: "╭─ <title> " + fill + "╮", total length = width.
  const fill = "─".repeat(Math.max(0, layout.width - layout.title.length - 5));

  return (
    <Box flexDirection="column" width={layout.width}>
      <Text color={palette.teal}>
        {layout.width < 5 ? (
          "─".repeat(layout.width)
        ) : (
          <>
            {"╭─ "}
            <Text bold color={palette.white}>
              {layout.title}
            </Text>
            {` ${fill}╮`}
          </>
        )}
      </Text>
      <Box
        flexDirection="column"
        width={layout.width}
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
