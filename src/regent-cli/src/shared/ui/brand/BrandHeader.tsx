import { PixelArt } from "@shared/ui/brand/PixelArt.tsx";
import { renderWordmark } from "@shared/ui/brand/art.ts";
// The "REGENT" wordmark — a bold, outlined teal pixel font (HERMES-style),
// rendered from per-cell half-block art.
import { Box } from "ink";

export function BrandHeader() {
  return (
    <Box marginTop={1} marginBottom={1}>
      <PixelArt rows={renderWordmark()} />
    </Box>
  );
}
