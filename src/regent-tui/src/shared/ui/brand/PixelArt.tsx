import type { ArtCell } from "@shared/ui/brand/art.ts";
// Renders half-block art cells (from the PNG converter or the wordmark) with
// per-cell truecolor fg/bg. Static art — index keys are intentional.
import { Box, Text } from "ink";

export function PixelArt({ rows }: { readonly rows: readonly (readonly ArtCell[])[] }) {
  return (
    <Box flexDirection="column">
      {rows.map((cells, i) => (
        <Text key={i}>
          {cells.map((cell, j) => {
            // Only set colours when present — Ink rejects explicit undefined
            // under exactOptionalPropertyTypes.
            const props: { color?: string; backgroundColor?: string } = {};
            if (cell.color) props.color = cell.color;
            if (cell.bg) props.backgroundColor = cell.bg;
            return (
              <Text key={j} {...props}>
                {cell.char}
              </Text>
            );
          })}
        </Text>
      ))}
    </Box>
  );
}
