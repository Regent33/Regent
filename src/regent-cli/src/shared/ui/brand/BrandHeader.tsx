import { REGENT_BANNER } from "@shared/ui/brand/art.ts";
import { tealShade } from "@shared/ui/tokens/theme.ts";
// The "REGENT" wordmark — the ANSI Shadow figlet font (chunky 3D block letters,
// HERMES-AGENT style), coloured with a top-to-bottom teal gradient.
import { Box, Text } from "ink";

export function BrandHeader() {
  return (
    <Box flexDirection="column" marginTop={1} marginBottom={1}>
      {REGENT_BANNER.map((line, i) => (
        <Text key={line} color={tealShade(i, REGENT_BANNER.length)}>
          {line}
        </Text>
      ))}
    </Box>
  );
}
